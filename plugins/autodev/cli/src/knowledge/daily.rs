use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::Result;

use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;

use super::models::{
    CrossAnalysis, DailyReport, DailySummary, KnowledgeSuggestion, Pattern, PatternType,
};

/// 데몬 로그 파싱 결과
#[derive(Debug, Default)]
pub struct LogStats {
    pub issues_done: u32,
    pub prs_done: u32,
    pub failed: u32,
    pub skipped: u32,
    pub total_duration_ms: u64,
    pub task_count: u32,
    pub error_lines: Vec<String>,
    pub task_ids: Vec<String>,
}

/// 데몬 로그 파일을 파싱하여 일간 통계 추출
pub fn parse_daemon_log(log_path: &Path) -> LogStats {
    let mut stats = LogStats::default();

    let file = match std::fs::File::open(log_path) {
        Ok(f) => f,
        Err(_) => return stats,
    };

    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok) {
        // "→ done" or "→ Done" — issue/PR 완료
        if line.contains("→ Done") || line.contains("→ done") {
            if line.contains("issue") || line.contains("Issue") {
                stats.issues_done += 1;
            } else if line.contains("PR") || line.contains("pr") {
                stats.prs_done += 1;
            }
            stats.task_count += 1;
        }

        // "→ Failed" — 실패
        if line.contains("→ Failed") || line.contains("→ failed") {
            stats.failed += 1;
            stats.task_count += 1;
        }

        // "→ skip" — 스킵
        if line.contains("→ skip") || line.contains("→ Skip") {
            stats.skipped += 1;
            stats.task_count += 1;
        }

        // duration 추출: "(1234ms)"
        if let Some(start) = line.rfind('(') {
            if let Some(end) = line.rfind("ms)") {
                if let Ok(ms) = line[start + 1..end].parse::<u64>() {
                    stats.total_duration_ms += ms;
                }
            }
        }

        // ERROR 라인 수집
        if line.contains(" ERROR ") || line.contains("[ERROR]") {
            stats.error_lines.push(line.clone());
        }

        // 태스크 ID 수집: "queued issue #42" or "queued PR #10"
        if line.contains("queued ") {
            if let Some(hash_pos) = line.find('#') {
                let num_str: String = line[hash_pos + 1..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if !num_str.is_empty() {
                    let prefix = if line.contains("issue") {
                        "issue"
                    } else {
                        "pr"
                    };
                    stats.task_ids.push(format!("{prefix}:{num_str}"));
                }
            }
        }
    }

    stats
}

/// 로그 통계에서 반복 패턴 감지
pub fn detect_patterns(stats: &LogStats) -> Vec<Pattern> {
    let mut patterns = Vec::new();

    // repeated_failure: 실패가 3건 이상이면 패턴으로 보고
    if stats.failed >= 3 {
        patterns.push(Pattern {
            pattern_type: PatternType::RepeatedFailure,
            description: format!("{} tasks failed in a single day", stats.failed),
            occurrences: stats.failed,
            affected_tasks: stats.task_ids.clone(),
        });
    }

    // error 빈도 기반 hotfile 추출 (같은 파일명이 error에 3회 이상 등장)
    let mut file_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for err_line in &stats.error_lines {
        // 파일명 패턴 추출: "src/..." or "*.rs" etc.
        for word in err_line.split_whitespace() {
            if (word.contains('/') && word.contains('.')) || word.ends_with(".rs") {
                *file_counts.entry(word.to_string()).or_default() += 1;
            }
        }
    }
    for (file, count) in &file_counts {
        if *count >= 3 {
            patterns.push(Pattern {
                pattern_type: PatternType::Hotfile,
                description: format!("{file} appeared in {count} error lines"),
                occurrences: *count,
                affected_tasks: vec![],
            });
        }
    }

    patterns
}

/// DailyReport 생성
pub fn build_daily_report(date: &str, stats: &LogStats, patterns: Vec<Pattern>) -> DailyReport {
    let avg_duration_ms = if stats.task_count > 0 {
        stats.total_duration_ms / stats.task_count as u64
    } else {
        0
    };

    DailyReport {
        date: date.to_string(),
        summary: DailySummary {
            issues_done: stats.issues_done,
            prs_done: stats.prs_done,
            failed: stats.failed,
            skipped: stats.skipped,
            avg_duration_ms,
        },
        patterns,
        suggestions: Vec::new(), // Claude가 채울 영역
        cross_analysis: None,    // suggest-workflow 연동 시 채워짐
    }
}

/// suggest-workflow에서 교차 분석 데이터를 수집하여 DailyReport에 추가
pub async fn enrich_with_cross_analysis(report: &mut DailyReport, sw: &dyn SuggestWorkflow) {
    let session_filter = "first_prompt_snippet LIKE '[autodev]%'";

    // 1. filtered-sessions: 전일 autodev 세션 목록
    let sessions = match sw
        .query_filtered_sessions("[autodev]", Some(&report.date), None)
        .await
    {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("suggest-workflow filtered-sessions query failed (non-fatal): {e}");
            Vec::new()
        }
    };

    // 2. tool-frequency: autodev 세션의 도구 사용 빈도
    let tool_frequencies = match sw.query_tool_frequency(Some(session_filter)).await {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!("suggest-workflow tool-frequency query failed (non-fatal): {e}");
            Vec::new()
        }
    };

    // 3. repetition: 이상치 세션 탐지
    let anomalies = match sw.query_repetition(Some(session_filter)).await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("suggest-workflow repetition query failed (non-fatal): {e}");
            Vec::new()
        }
    };

    // 데이터가 하나라도 있으면 cross_analysis 설정
    if !sessions.is_empty() || !tool_frequencies.is_empty() || !anomalies.is_empty() {
        report.cross_analysis = Some(CrossAnalysis {
            tool_frequencies,
            anomalies,
            sessions,
        });
    }
}

/// Claude에게 DailyReport를 전달하여 KnowledgeSuggestion 생성
///
/// report에 cross_analysis가 포함되어 있으면 교차 분석 섹션도 프롬프트에 포함.
pub async fn generate_daily_suggestions(
    claude: &dyn Claude,
    report: &DailyReport,
    wt_path: &Path,
) -> Option<KnowledgeSuggestion> {
    let report_json = match serde_json::to_string_pretty(report) {
        Ok(j) => j,
        Err(e) => {
            tracing::error!("failed to serialize daily report: {e}");
            return None;
        }
    };

    let cross_analysis_hint = if report.cross_analysis.is_some() {
        "\n\nThe report includes `cross_analysis` from suggest-workflow with tool usage patterns, \
         session data, and anomalies. Cross-reference these with daemon log patterns to identify \
         deeper insights (e.g., high tool frequency correlated with failures, repeated test loops)."
    } else {
        ""
    };

    let date = &report.date;
    let prompt = format!(
        "[autodev] knowledge: daily {date}\n\n\
         Below is today's autodev daily report. Analyze the patterns and suggest improvements.\n\n\
         ```json\n{report_json}\n```{cross_analysis_hint}\n\n\
         Respond with a JSON object:\n\
         {{\n  \"suggestions\": [\n    {{\n      \
         \"type\": \"rule | claude_md | hook | skill | subagent\",\n      \
         \"target_file\": \"path to file\",\n      \
         \"content\": \"specific recommendation\",\n      \
         \"reason\": \"why this matters\"\n    }}\n  ]\n}}"
    );

    match claude
        .run_session(wt_path, &prompt, &Default::default())
        .await
    {
        Ok(res) if res.exit_code == 0 => {
            // envelope → inner parse
            if let Ok(envelope) = serde_json::from_str::<
                crate::infrastructure::claude::output::ClaudeJsonOutput,
            >(&res.stdout)
            {
                if let Some(inner) = envelope.result {
                    if let Ok(ks) = serde_json::from_str::<KnowledgeSuggestion>(&inner) {
                        return Some(ks);
                    }
                }
            }
            serde_json::from_str::<KnowledgeSuggestion>(&res.stdout).ok()
        }
        Ok(res) => {
            tracing::warn!("daily suggestion generation exited with {}", res.exit_code);
            None
        }
        Err(e) => {
            tracing::warn!("daily suggestion generation failed: {e}");
            None
        }
    }
}

/// 일간 리포트를 GitHub 이슈로 게시
pub async fn post_daily_report(
    gh: &dyn Gh,
    repo_name: &str,
    report: &DailyReport,
    gh_host: Option<&str>,
) {
    let title = format!("[autodev] Daily Report {}", report.date);
    let body = format_daily_report_body(report);

    // gh api로 이슈 생성
    let created = gh.create_issue(repo_name, &title, &body, gh_host).await;
    if !created {
        tracing::error!("failed to create daily report issue for {repo_name}");
    }
}

/// DailyReport의 suggestions를 각각 PR로 생성
///
/// 각 suggestion에 대해:
/// 1. branch 생성 (autodev/knowledge/{date}-{index})
/// 2. target_file에 content 기록
/// 3. commit + push
/// 4. PR 생성 + autodev:skip 라벨 부착
pub async fn create_knowledge_prs(
    gh: &dyn Gh,
    git: &dyn crate::infrastructure::git::Git,
    repo_name: &str,
    report: &DailyReport,
    base_path: &Path,
    gh_host: Option<&str>,
) {
    use crate::queue::task_queues::labels;

    for (i, suggestion) in report.suggestions.iter().enumerate() {
        let branch = format!("autodev/knowledge/{}-{}", report.date, i);
        let target = &suggestion.target_file;

        // 1. branch 생성
        if let Err(e) = git.checkout_new_branch(base_path, &branch).await {
            tracing::warn!("knowledge PR: failed to create branch {branch}: {e}");
            continue;
        }

        // 2. 파일 쓰기
        let file_path = base_path.join(target);
        if let Some(parent) = file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&file_path, &suggestion.content) {
            tracing::warn!("knowledge PR: failed to write {target}: {e}");
            continue;
        }

        // 3. commit + push
        let message = format!("[autodev] knowledge: {}", suggestion.reason);
        if let Err(e) = git
            .add_commit_push(base_path, &[target.as_str()], &message, &branch)
            .await
        {
            tracing::warn!("knowledge PR: failed to commit+push {branch}: {e}");
            continue;
        }

        // 4. PR 생성
        let pr_title = format!("[autodev] rule: {}", suggestion.reason);
        let pr_body = format!(
            "<!-- autodev:knowledge-pr -->\n\n\
             ## Knowledge Suggestion\n\n\
             **Type**: {:?}\n\
             **Target**: `{}`\n\n\
             ### Content\n\n```\n{}\n```\n\n\
             ### Reason\n\n{}",
            suggestion.suggestion_type,
            suggestion.target_file,
            suggestion.content,
            suggestion.reason,
        );

        if let Some(pr_number) = gh
            .create_pr(repo_name, &branch, "main", &pr_title, &pr_body, gh_host)
            .await
        {
            // autodev:skip 라벨 부착 — 스캐너가 자동 처리하지 않도록
            gh.label_add(repo_name, pr_number, labels::SKIP, gh_host)
                .await;
            tracing::info!(
                "knowledge PR #{pr_number} created for suggestion {i}: {}",
                suggestion.reason
            );
        } else {
            tracing::warn!("knowledge PR: failed to create PR for suggestion {i}");
        }
    }
}

/// DailyReport를 Markdown 본문으로 포맷
fn format_daily_report_body(report: &DailyReport) -> String {
    let s = &report.summary;
    let mut body = format!(
        "<!-- autodev:daily-report -->\n\
         ## Summary\n\n\
         | Metric | Count |\n\
         |--------|-------|\n\
         | Issues done | {} |\n\
         | PRs done | {} |\n\
         | Failed | {} |\n\
         | Skipped | {} |\n\
         | Avg duration | {}ms |\n",
        s.issues_done, s.prs_done, s.failed, s.skipped, s.avg_duration_ms
    );

    if !report.patterns.is_empty() {
        body.push_str("\n## Patterns\n\n");
        for p in &report.patterns {
            body.push_str(&format!(
                "- **{:?}** (x{}): {}\n",
                p.pattern_type, p.occurrences, p.description
            ));
            if !p.affected_tasks.is_empty() {
                body.push_str(&format!("  - Affected: {}\n", p.affected_tasks.join(", ")));
            }
        }
    }

    if let Some(ref ca) = report.cross_analysis {
        body.push_str("\n## Cross Analysis (suggest-workflow)\n\n");

        if !ca.sessions.is_empty() {
            body.push_str(&format!(
                "**Sessions**: {} autodev sessions\n\n",
                ca.sessions.len()
            ));
        }

        if !ca.tool_frequencies.is_empty() {
            body.push_str("**Top Tools**:\n");
            for tf in ca.tool_frequencies.iter().take(10) {
                body.push_str(&format!(
                    "- `{}`: {} uses across {} sessions\n",
                    tf.tool, tf.frequency, tf.sessions
                ));
            }
            body.push('\n');
        }

        if !ca.anomalies.is_empty() {
            body.push_str("**Anomalies** (z-score outliers):\n");
            for a in &ca.anomalies {
                body.push_str(&format!(
                    "- Session `{}`: `{}` x{} (deviation: {:.1})\n",
                    a.session_id, a.tool, a.cnt, a.deviation_score
                ));
            }
            body.push('\n');
        }
    }

    if !report.suggestions.is_empty() {
        body.push_str("\n## Suggestions\n\n");
        for s in &report.suggestions {
            body.push_str(&format!(
                "- **{:?}** → `{}`: {}\n  > {}\n",
                s.suggestion_type, s.target_file, s.content, s.reason
            ));
        }
    }

    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn parse_daemon_log_counts_done_and_failed() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("daemon.2026-02-22.log");
        {
            let mut f = std::fs::File::create(&log_path).unwrap();
            writeln!(f, "2026-02-22T10:00:00 INFO issue #42 → Done (5000ms)").unwrap();
            writeln!(f, "2026-02-22T10:01:00 INFO PR #10 → Done (3000ms)").unwrap();
            writeln!(f, "2026-02-22T10:02:00 ERROR issue #43 → Failed").unwrap();
            writeln!(f, "2026-02-22T10:03:00 INFO issue #44 → skip").unwrap();
            writeln!(f, "2026-02-22T09:00:00 INFO queued issue #42").unwrap();
            writeln!(f, "2026-02-22T09:01:00 INFO queued PR #10").unwrap();
        }

        let stats = parse_daemon_log(&log_path);
        assert_eq!(stats.issues_done, 1);
        assert_eq!(stats.prs_done, 1);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.skipped, 1);
        assert_eq!(stats.total_duration_ms, 8000);
        assert_eq!(stats.task_count, 4);
        assert_eq!(stats.task_ids.len(), 2);
    }

    #[test]
    fn parse_daemon_log_empty_file() {
        let tmp = TempDir::new().unwrap();
        let log_path = tmp.path().join("daemon.log");
        std::fs::File::create(&log_path).unwrap();

        let stats = parse_daemon_log(&log_path);
        assert_eq!(stats.task_count, 0);
    }

    #[test]
    fn parse_daemon_log_missing_file() {
        let stats = parse_daemon_log(Path::new("/nonexistent/log.log"));
        assert_eq!(stats.task_count, 0);
    }

    #[test]
    fn detect_patterns_repeated_failure() {
        let stats = LogStats {
            failed: 3,
            task_ids: vec!["issue:1".into(), "issue:2".into(), "issue:3".into()],
            ..Default::default()
        };

        let patterns = detect_patterns(&stats);
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].pattern_type, PatternType::RepeatedFailure);
        assert_eq!(patterns[0].occurrences, 3);
    }

    #[test]
    fn detect_patterns_no_patterns_when_few_failures() {
        let stats = LogStats {
            failed: 1,
            ..Default::default()
        };

        let patterns = detect_patterns(&stats);
        assert!(patterns.is_empty());
    }

    #[test]
    fn build_daily_report_computes_avg_duration() {
        let stats = LogStats {
            issues_done: 5,
            prs_done: 2,
            failed: 1,
            skipped: 0,
            total_duration_ms: 8000,
            task_count: 8,
            ..Default::default()
        };

        let report = build_daily_report("2026-02-22", &stats, vec![]);
        assert_eq!(report.summary.avg_duration_ms, 1000);
        assert_eq!(report.summary.issues_done, 5);
        assert_eq!(report.date, "2026-02-22");
    }

    #[test]
    fn build_daily_report_zero_tasks() {
        let stats = LogStats::default();
        let report = build_daily_report("2026-02-22", &stats, vec![]);
        assert_eq!(report.summary.avg_duration_ms, 0);
    }

    #[test]
    fn format_daily_report_body_renders_markdown() {
        let report = DailyReport {
            date: "2026-02-22".into(),
            summary: DailySummary {
                issues_done: 3,
                prs_done: 1,
                failed: 0,
                skipped: 1,
                avg_duration_ms: 5000,
            },
            patterns: vec![Pattern {
                pattern_type: PatternType::RepeatedFailure,
                description: "3 tasks failed".into(),
                occurrences: 3,
                affected_tasks: vec!["issue:1".into()],
            }],
            suggestions: vec![],
            cross_analysis: None,
        };

        let body = format_daily_report_body(&report);
        assert!(body.contains("autodev:daily-report"));
        assert!(body.contains("Issues done | 3"));
        assert!(body.contains("RepeatedFailure"));
    }
}
