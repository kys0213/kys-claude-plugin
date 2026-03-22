use anyhow::Result;

use crate::v5::core::context::ItemContext;
use crate::v5::core::datasource::DataSource;
use crate::v5::core::queue_item::V5QueueItem;

/// `autodev context <work_id> --json` 핸들러.
///
/// DataSource.get_context()로 ItemContext를 구성하고 JSON으로 출력한다.
/// on_done/on_fail script에서 사용.
pub async fn context_json(source: &dyn DataSource, item: &V5QueueItem) -> Result<String> {
    let ctx = source.get_context(item).await?;
    Ok(serde_json::to_string_pretty(&ctx)?)
}

/// `autodev context <work_id> --field <path>` 핸들러.
///
/// ItemContext에서 특정 필드를 jq-like dotted path로 추출한다.
/// e.g. "issue.number" → "42"
pub fn extract_field(ctx: &ItemContext, field_path: &str) -> Result<String> {
    let value = serde_json::to_value(ctx)?;
    let parts: Vec<&str> = field_path.split('.').collect();
    let mut current = &value;

    for part in &parts {
        current = current
            .get(part)
            .ok_or_else(|| anyhow::anyhow!("field not found: {field_path}"))?;
    }

    match current {
        serde_json::Value::String(s) => Ok(s.clone()),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        serde_json::Value::Null => Ok("null".to_string()),
        other => Ok(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v5::core::context::*;

    fn test_context() -> ItemContext {
        ItemContext {
            work_id: "github:org/repo#42:implement".to_string(),
            workspace: "test-ws".to_string(),
            queue: QueueContext {
                phase: "running".to_string(),
                state: "implement".to_string(),
                source_id: "github:org/repo#42".to_string(),
            },
            source: SourceContext {
                source_type: "github".to_string(),
                url: "https://github.com/org/repo".to_string(),
                default_branch: Some("main".to_string()),
            },
            issue: Some(IssueContext {
                number: 42,
                title: "JWT middleware".to_string(),
                body: None,
                labels: vec!["autodev:implement".to_string()],
                author: "irene".to_string(),
            }),
            pr: None,
            history: vec![],
            worktree: Some("/tmp/autodev/test-42".to_string()),
        }
    }

    #[test]
    fn extract_field_simple() {
        let ctx = test_context();
        assert_eq!(
            extract_field(&ctx, "work_id").unwrap(),
            "github:org/repo#42:implement"
        );
        assert_eq!(extract_field(&ctx, "workspace").unwrap(), "test-ws");
    }

    #[test]
    fn extract_field_nested() {
        let ctx = test_context();
        assert_eq!(extract_field(&ctx, "issue.number").unwrap(), "42");
        assert_eq!(extract_field(&ctx, "issue.author").unwrap(), "irene");
        assert_eq!(
            extract_field(&ctx, "source.url").unwrap(),
            "https://github.com/org/repo"
        );
    }

    #[test]
    fn extract_field_not_found() {
        let ctx = test_context();
        assert!(extract_field(&ctx, "nonexistent").is_err());
        assert!(extract_field(&ctx, "issue.nonexistent").is_err());
    }

    #[test]
    fn extract_field_queue_state() {
        let ctx = test_context();
        assert_eq!(extract_field(&ctx, "queue.state").unwrap(), "implement");
        assert_eq!(
            extract_field(&ctx, "queue.source_id").unwrap(),
            "github:org/repo#42"
        );
    }
}
