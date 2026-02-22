use std::path::Path;

use anyhow::Result;

use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;

use super::models::KnowledgeSuggestion;

/// per-task knowledge extraction
///
/// done 전이 시 호출: 완료된 작업의 세션을 Claude로 분석하여 개선 제안 추출.
/// 결과는 GitHub 이슈 코멘트로 게시.
pub async fn extract_task_knowledge(
    claude: &dyn Claude,
    gh: &dyn Gh,
    repo_name: &str,
    github_number: i64,
    task_type: &str,
    wt_path: &Path,
    gh_host: Option<&str>,
) -> Result<Option<KnowledgeSuggestion>> {
    let prompt = format!(
        "[autodev] knowledge: per-task {task_type} #{github_number}\n\n\
         Analyze the completed {task_type} task (#{github_number}) in this workspace. \
         Review the changes made, any issues encountered, and lessons learned. \
         Respond with a JSON object matching this schema:\n\
         {{\n  \"suggestions\": [\n    {{\n      \
         \"type\": \"rule | claude_md | hook | skill | subagent\",\n      \
         \"target_file\": \".claude/rules/...\",\n      \
         \"content\": \"specific recommendation\",\n      \
         \"reason\": \"why this matters\"\n    }}\n  ]\n}}\n\n\
         Only include suggestions if there are genuine improvements to propose. \
         If none, return {{\"suggestions\": []}}."
    );

    let result = claude.run_session(wt_path, &prompt, None).await;

    let suggestion = match result {
        Ok(res) if res.exit_code == 0 => parse_knowledge_suggestion(&res.stdout),
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

    // 제안이 있으면 GitHub 코멘트로 게시
    if let Some(ref ks) = suggestion {
        if !ks.suggestions.is_empty() {
            let comment = format_knowledge_comment(ks, task_type, github_number);
            gh.issue_comment(repo_name, github_number, &comment, gh_host)
                .await;
        }
    }

    Ok(suggestion)
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
