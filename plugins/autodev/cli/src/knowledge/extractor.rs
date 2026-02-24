use std::path::Path;

use anyhow::Result;

use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::queue::task_queues::labels;

use super::models::KnowledgeSuggestion;

/// v2: worktree에서 기존 지식 베이스를 문자열로 수집
///
/// CLAUDE.md, .claude/rules/*.md 등 기존 지식 파일의 내용을 모아서
/// delta check 프롬프트에 사용한다.
pub fn collect_existing_knowledge(wt_path: &Path) -> String {
    let mut knowledge = String::new();

    // CLAUDE.md
    let claude_md = wt_path.join("CLAUDE.md");
    if claude_md.exists() {
        if let Ok(content) = std::fs::read_to_string(&claude_md) {
            knowledge.push_str("--- CLAUDE.md ---\n");
            knowledge.push_str(&content);
            knowledge.push_str("\n\n");
        }
    }

    // .claude/rules/*.md
    let rules_dir = wt_path.join(".claude/rules");
    collect_md_dir(&rules_dir, ".claude/rules", &mut knowledge);

    // .claude/hooks.json
    let hooks_json = wt_path.join(".claude/hooks.json");
    if hooks_json.exists() {
        if let Ok(content) = std::fs::read_to_string(&hooks_json) {
            knowledge.push_str("--- .claude/hooks.json ---\n");
            knowledge.push_str(&content);
            knowledge.push_str("\n\n");
        }
    }

    // .claude-plugin/ (skills, plugin.json)
    let plugin_dir = wt_path.join(".claude-plugin");
    if plugin_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&plugin_dir) {
            let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            paths.sort_by_key(|e| e.path());
            for entry in paths {
                let path = entry.path();
                if path.is_file() {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let name = path.file_name().unwrap_or_default().to_string_lossy();
                        knowledge.push_str(&format!("--- .claude-plugin/{name} ---\n"));
                        knowledge.push_str(&content);
                        knowledge.push_str("\n\n");
                    }
                }
            }
        }
    }

    // .develop-workflow.yaml (워크플로우 설정)
    let workflow_yaml = wt_path.join(".develop-workflow.yaml");
    if workflow_yaml.exists() {
        if let Ok(content) = std::fs::read_to_string(&workflow_yaml) {
            knowledge.push_str("--- .develop-workflow.yaml ---\n");
            knowledge.push_str(&content);
            knowledge.push_str("\n\n");
        }
    }

    knowledge
}

/// 디렉토리 내 *.md 파일을 수집하여 knowledge 문자열에 추가
fn collect_md_dir(dir: &Path, label: &str, knowledge: &mut String) {
    if dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
                .collect();
            paths.sort_by_key(|e| e.path());

            for entry in paths {
                let path = entry.path();
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    knowledge.push_str(&format!("--- {label}/{name} ---\n"));
                    knowledge.push_str(&content);
                    knowledge.push_str("\n\n");
                }
            }
        }
    }
}

/// per-task knowledge extraction
///
/// done 전이 시 호출: 완료된 작업의 세션을 Claude로 분석하여 개선 제안 추출.
/// suggest-workflow 세션 데이터가 있으면 도구 사용 패턴도 함께 분석.
/// 결과는 GitHub 이슈 코멘트로 게시.
#[allow(clippy::too_many_arguments)]
pub async fn extract_task_knowledge(
    claude: &dyn Claude,
    gh: &dyn Gh,
    git: &dyn Git,
    sw: &dyn SuggestWorkflow,
    repo_name: &str,
    github_number: i64,
    task_type: &str,
    wt_path: &Path,
    gh_host: Option<&str>,
) -> Result<Option<KnowledgeSuggestion>> {
    // suggest-workflow에서 해당 태스크 세션의 도구 사용 패턴 조회 (best effort)
    let sw_section = build_suggest_workflow_section(sw, task_type, github_number).await;

    // v2: delta-aware — 기존 지식과 비교하여 중복 제거
    let existing = collect_existing_knowledge(wt_path);
    let delta_section = if existing.is_empty() {
        String::new()
    } else {
        format!(
            "\n\n--- Existing Knowledge Base ---\n\
             The following knowledge already exists in this repository. \
             Do NOT suggest anything that is already covered below. \
             Only suggest genuinely NEW improvements.\n\n{existing}"
        )
    };

    let prompt = format!(
        "[autodev] knowledge: per-task {task_type} #{github_number}\n\n\
         Analyze the completed {task_type} task (#{github_number}) in this workspace. \
         Review the changes made, any issues encountered, and lessons learned.\
         {sw_section}{delta_section}\n\n\
         Respond with a JSON object matching this schema:\n\
         {{\n  \"suggestions\": [\n    {{\n      \
         \"type\": \"rule | claude_md | hook | skill | subagent\",\n      \
         \"target_file\": \".claude/rules/...\",\n      \
         \"content\": \"specific recommendation\",\n      \
         \"reason\": \"why this matters\"\n    }}\n  ]\n}}\n\n\
         Only include suggestions if there are genuine improvements to propose. \
         If none, return {{\"suggestions\": []}}."
    );

    let result = claude
        .run_session(wt_path, &prompt, &Default::default())
        .await;

    let suggestion = match result {
        Ok(res) if res.exit_code == 0 => {
            // empty suggestions → None (코멘트 게시 불필요)
            parse_knowledge_suggestion(&res.stdout).filter(|ks| !ks.suggestions.is_empty())
        }
        Ok(res) => {
            tracing::warn!(
                "knowledge extraction exited with {} for {task_type} #{github_number}",
                res.exit_code
            );
            None
        }
        Err(e) => {
            tracing::warn!("knowledge extraction failed for {task_type} #{github_number}: {e}");
            None
        }
    };

    // 제안이 있으면 GitHub 코멘트로 게시 + per-task actionable PR 생성
    if let Some(ref ks) = suggestion {
        let comment = format_knowledge_comment(ks, task_type, github_number);
        gh.issue_comment(repo_name, github_number, &comment, gh_host)
            .await;

        // per-task actionable PR: suggestion마다 PR 생성
        create_task_knowledge_prs(
            gh,
            git,
            repo_name,
            ks,
            task_type,
            github_number,
            wt_path,
            gh_host,
        )
        .await;
    }

    Ok(suggestion)
}

/// suggest-workflow에서 해당 태스크 세션의 도구 사용 패턴을 조회하여 프롬프트 섹션 생성
async fn build_suggest_workflow_section(
    sw: &dyn SuggestWorkflow,
    task_type: &str,
    github_number: i64,
) -> String {
    // autodev 세션 식별: "[autodev]" 마커 + task 키워드로 필터
    let session_filter =
        format!("first_prompt_snippet LIKE '[autodev]%{task_type}%#{github_number}%'");

    let tool_freq = match sw.query_tool_frequency(Some(&session_filter)).await {
        Ok(entries) if !entries.is_empty() => entries,
        Ok(_) => return String::new(),
        Err(e) => {
            tracing::debug!("suggest-workflow tool-frequency query failed (non-fatal): {e}");
            return String::new();
        }
    };

    let tool_freq_json = match serde_json::to_string_pretty(&tool_freq) {
        Ok(j) => j,
        Err(_) => return String::new(),
    };

    format!(
        "\n\n--- suggest-workflow session data ---\n\
         The following tool usage pattern was recorded for this task's session:\n\
         ```json\n{tool_freq_json}\n```\n\
         Consider these patterns when making suggestions \
         (e.g., high Bash:test frequency may indicate test loop issues)."
    )
}

/// Claude 출력에서 KnowledgeSuggestion 파싱
fn parse_knowledge_suggestion(stdout: &str) -> Option<KnowledgeSuggestion> {
    // claude --output-format json envelope
    if let Ok(envelope) =
        serde_json::from_str::<crate::infrastructure::claude::output::ClaudeJsonOutput>(stdout)
    {
        if let Some(inner) = envelope.result {
            if let Ok(ks) = serde_json::from_str::<KnowledgeSuggestion>(&inner) {
                return Some(ks);
            }
        }
    }
    // 직접 파싱
    serde_json::from_str::<KnowledgeSuggestion>(stdout).ok()
}

/// KnowledgeSuggestion을 GitHub 코멘트로 포맷
fn format_knowledge_comment(ks: &KnowledgeSuggestion, task_type: &str, number: i64) -> String {
    let mut comment =
        format!("<!-- autodev:knowledge -->\n## Autodev Knowledge ({task_type} #{number})\n\n");

    for (i, s) in ks.suggestions.iter().enumerate() {
        comment.push_str(&format!(
            "### {}. `{:?}` → `{}`\n\n{}\n\n> **Reason**: {}\n\n",
            i + 1,
            s.suggestion_type,
            s.target_file,
            s.content,
            s.reason
        ));
    }

    comment
}

/// per-task suggestion마다 actionable PR 생성
///
/// 각 suggestion에 대해 branch → file write → commit → push → PR 생성.
/// daily knowledge PR과 동일한 패턴이지만 branch naming이 task 단위.
#[allow(clippy::too_many_arguments)]
async fn create_task_knowledge_prs(
    gh: &dyn Gh,
    git: &dyn Git,
    repo_name: &str,
    ks: &KnowledgeSuggestion,
    task_type: &str,
    github_number: i64,
    base_path: &Path,
    gh_host: Option<&str>,
) {
    let today = chrono::Local::now().format("%Y-%m-%d");
    for (i, suggestion) in ks.suggestions.iter().enumerate() {
        let branch = format!("autodev/knowledge/{task_type}-{github_number}-{today}-{i}");
        let target = &suggestion.target_file;

        if let Err(e) = git.checkout_new_branch(base_path, &branch).await {
            tracing::warn!("task knowledge PR: failed to create branch {branch}: {e}");
            continue;
        }

        let file_path = base_path.join(target);
        if let Some(parent) = file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&file_path, &suggestion.content) {
            tracing::warn!("task knowledge PR: failed to write {target}: {e}");
            continue;
        }

        let message = format!("[autodev] knowledge: {}", suggestion.reason);
        if let Err(e) = git
            .add_commit_push(base_path, &[target.as_str()], &message, &branch)
            .await
        {
            tracing::warn!("task knowledge PR: failed to commit+push {branch}: {e}");
            continue;
        }

        let pr_title = format!("[autodev] rule: {}", suggestion.reason);
        let pr_body = format!(
            "<!-- autodev:knowledge-pr -->\n\n\
             ## Knowledge Suggestion ({task_type} #{github_number})\n\n\
             **Type**: {:?}\n\
             **Target**: `{}`\n\n\
             ### Content\n\n```\n{}\n```\n\n\
             ### Reason\n\n{}",
            suggestion.suggestion_type,
            suggestion.target_file,
            suggestion.content,
            suggestion.reason,
        );

        if let Some(pr_num) = gh
            .create_pr(repo_name, &branch, "main", &pr_title, &pr_body, gh_host)
            .await
        {
            gh.label_add(repo_name, pr_num, labels::SKIP, gh_host).await;
            tracing::info!(
                "task knowledge PR #{pr_num} created for {task_type} #{github_number} suggestion {i}"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_knowledge_suggestion_from_raw_json() {
        let json = r#"{"suggestions":[{"type":"rule","target_file":".claude/rules/test.md","content":"Always run tests","reason":"Tests caught 3 bugs"}]}"#;
        let ks = parse_knowledge_suggestion(json).unwrap();
        assert_eq!(ks.suggestions.len(), 1);
        assert_eq!(
            ks.suggestions[0].suggestion_type,
            super::super::models::SuggestionType::Rule
        );
        assert_eq!(ks.suggestions[0].target_file, ".claude/rules/test.md");
    }

    #[test]
    fn parse_knowledge_suggestion_empty() {
        let json = r#"{"suggestions":[]}"#;
        let ks = parse_knowledge_suggestion(json).unwrap();
        assert!(ks.suggestions.is_empty());
    }

    #[test]
    fn parse_knowledge_suggestion_from_envelope() {
        let inner = r#"{"suggestions":[{"type":"hook","target_file":".claude/hooks.json","content":"Add linter hook","reason":"Consistent formatting"}]}"#;
        let envelope = format!(r#"{{"result":"{}"}}"#, inner.replace('"', "\\\""));
        let ks = parse_knowledge_suggestion(&envelope).unwrap();
        assert_eq!(ks.suggestions.len(), 1);
    }

    #[test]
    fn parse_knowledge_suggestion_invalid_returns_none() {
        assert!(parse_knowledge_suggestion("not json").is_none());
    }

    #[test]
    fn collect_existing_knowledge_reads_claude_md_and_rules() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();

        // Create CLAUDE.md
        std::fs::write(base.join("CLAUDE.md"), "# My Rules\nBe careful").unwrap();

        // Create .claude/rules/
        std::fs::create_dir_all(base.join(".claude/rules")).unwrap();
        std::fs::write(base.join(".claude/rules/test.md"), "Always run tests").unwrap();
        std::fs::write(base.join(".claude/rules/lint.md"), "Run clippy").unwrap();

        let knowledge = collect_existing_knowledge(base);
        assert!(knowledge.contains("CLAUDE.md"));
        assert!(knowledge.contains("Be careful"));
        assert!(knowledge.contains("Always run tests"));
        assert!(knowledge.contains("Run clippy"));
        assert!(knowledge.contains(".claude/rules/lint.md"));
        assert!(knowledge.contains(".claude/rules/test.md"));
    }

    #[test]
    fn collect_existing_knowledge_reads_hooks_and_plugin() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();

        // .claude/hooks.json
        std::fs::create_dir_all(base.join(".claude")).unwrap();
        std::fs::write(base.join(".claude/hooks.json"), r#"{"hooks":[]}"#).unwrap();

        // .claude-plugin/plugin.json
        std::fs::create_dir_all(base.join(".claude-plugin")).unwrap();
        std::fs::write(
            base.join(".claude-plugin/plugin.json"),
            r#"{"name":"test"}"#,
        )
        .unwrap();

        // .develop-workflow.yaml
        std::fs::write(base.join(".develop-workflow.yaml"), "workflow: test").unwrap();

        let knowledge = collect_existing_knowledge(base);
        assert!(knowledge.contains(".claude/hooks.json"));
        assert!(knowledge.contains(r#"{"hooks":[]}"#));
        assert!(knowledge.contains(".claude-plugin/plugin.json"));
        assert!(knowledge.contains("workflow: test"));
        assert!(knowledge.contains(".develop-workflow.yaml"));
    }

    #[test]
    fn collect_existing_knowledge_empty_when_no_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        let knowledge = collect_existing_knowledge(tmp.path());
        assert!(knowledge.is_empty());
    }

    #[test]
    fn format_knowledge_comment_renders_properly() {
        let ks = KnowledgeSuggestion {
            suggestions: vec![super::super::models::Suggestion {
                suggestion_type: super::super::models::SuggestionType::Rule,
                target_file: ".claude/rules/test.md".into(),
                content: "Always run tests".into(),
                reason: "Caught 3 bugs".into(),
            }],
        };
        let comment = format_knowledge_comment(&ks, "issue", 42);
        assert!(comment.contains("autodev:knowledge"));
        assert!(comment.contains("issue #42"));
        assert!(comment.contains("Always run tests"));
        assert!(comment.contains("Caught 3 bugs"));
    }
}
