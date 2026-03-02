use std::path::Path;

use crate::components::workspace::Workspace;
use crate::domain::labels;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;

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

    // plugins/*/commands/*.md (skill 정의)
    let plugins_dir = wt_path.join("plugins");
    if plugins_dir.is_dir() {
        if let Ok(plugin_entries) = std::fs::read_dir(&plugins_dir) {
            let mut skill_lines = Vec::new();
            for plugin_entry in plugin_entries.flatten() {
                let cmds_dir = plugin_entry.path().join("commands");
                if cmds_dir.is_dir() {
                    if let Ok(cmd_entries) = std::fs::read_dir(&cmds_dir) {
                        for cmd in cmd_entries.flatten() {
                            if cmd.path().extension().is_some_and(|e| e == "md") {
                                let rel = cmd
                                    .path()
                                    .strip_prefix(wt_path)
                                    .unwrap_or(&cmd.path())
                                    .display()
                                    .to_string();
                                skill_lines.push(rel);
                            }
                        }
                    }
                }
            }
            if !skill_lines.is_empty() {
                skill_lines.sort();
                knowledge.push_str("--- Existing Skills ---\n");
                for line in &skill_lines {
                    knowledge.push_str(&format!("- {line}\n"));
                }
                knowledge.push('\n');
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

/// suggest-workflow에서 해당 태스크 세션의 도구 사용 패턴을 조회하여 프롬프트 섹션 생성
pub async fn build_suggest_workflow_section(
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
pub fn parse_knowledge_suggestion(stdout: &str) -> Option<KnowledgeSuggestion> {
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
pub fn format_knowledge_comment(ks: &KnowledgeSuggestion, task_type: &str, number: i64) -> String {
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

/// per-task knowledge PR 생성 시 필요한 컨텍스트.
pub struct KnowledgePrContext<'a> {
    pub repo_name: &'a str,
    pub task_type: &'a str,
    pub github_number: i64,
    pub gh_host: Option<&'a str>,
}

/// per-task suggestion마다 actionable PR 생성 (격리된 worktree 사용)
///
/// 각 suggestion에 대해 main 기반 별도 worktree → branch → file write → commit → push → PR 생성.
/// 구현 worktree와 격리하여 uncommitted 변경 충돌을 방지한다.
pub async fn create_task_knowledge_prs(
    gh: &dyn Gh,
    workspace: &Workspace<'_>,
    ctx: &KnowledgePrContext<'_>,
    ks: &KnowledgeSuggestion,
) {
    let git = workspace.git();
    let today = chrono::Local::now().format("%Y-%m-%d");
    let task_type = ctx.task_type;
    let github_number = ctx.github_number;
    let repo_name = ctx.repo_name;
    let gh_host = ctx.gh_host;
    for (i, suggestion) in ks.suggestions.iter().enumerate() {
        let branch = format!("autodev/knowledge/{task_type}-{github_number}-{today}-{i}");
        let knowledge_task_id = format!("knowledge-{task_type}-{github_number}-{i}");
        let target = &suggestion.target_file;

        // main 기반 별도 worktree 생성 (구현 worktree와 격리)
        let kn_wt_path = match workspace
            .create_worktree(repo_name, &knowledge_task_id, None)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("task knowledge PR: worktree creation failed: {e}");
                continue;
            }
        };

        if let Err(e) = git.checkout_new_branch(&kn_wt_path, &branch).await {
            tracing::warn!("task knowledge PR: failed to create branch {branch}: {e}");
            let _ = workspace
                .remove_worktree(repo_name, &knowledge_task_id)
                .await;
            continue;
        }

        let file_path = match crate::config::safe_join(&kn_wt_path, target) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("task knowledge PR: unsafe target_file '{target}': {e}");
                let _ = workspace
                    .remove_worktree(repo_name, &knowledge_task_id)
                    .await;
                continue;
            }
        };
        if let Some(parent) = file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = std::fs::write(&file_path, &suggestion.content) {
            tracing::warn!("task knowledge PR: failed to write {target}: {e}");
            let _ = workspace
                .remove_worktree(repo_name, &knowledge_task_id)
                .await;
            continue;
        }

        let message = format!("[autodev] knowledge: {}", suggestion.reason);
        if let Err(e) = git
            .add_commit_push(&kn_wt_path, &[target.as_str()], &message, &branch)
            .await
        {
            tracing::warn!("task knowledge PR: failed to commit+push {branch}: {e}");
            let _ = workspace
                .remove_worktree(repo_name, &knowledge_task_id)
                .await;
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

        // worktree 정리
        let _ = workspace
            .remove_worktree(repo_name, &knowledge_task_id)
            .await;
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
    fn collect_existing_knowledge_reads_plugin_skills() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path();

        // plugins/my-plugin/commands/foo.md
        std::fs::create_dir_all(base.join("plugins/my-plugin/commands")).unwrap();
        std::fs::write(
            base.join("plugins/my-plugin/commands/foo.md"),
            "# Foo skill",
        )
        .unwrap();
        std::fs::write(
            base.join("plugins/my-plugin/commands/bar.md"),
            "# Bar skill",
        )
        .unwrap();

        // plugins/other/commands/baz.md
        std::fs::create_dir_all(base.join("plugins/other/commands")).unwrap();
        std::fs::write(base.join("plugins/other/commands/baz.md"), "# Baz skill").unwrap();

        let knowledge = collect_existing_knowledge(base);
        assert!(knowledge.contains("Existing Skills"));
        assert!(knowledge.contains("plugins/my-plugin/commands/foo.md"));
        assert!(knowledge.contains("plugins/my-plugin/commands/bar.md"));
        assert!(knowledge.contains("plugins/other/commands/baz.md"));
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
