use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::infra::gh::Gh;
use crate::v5::core::context::{IssueContext, ItemContext, QueueContext, SourceContext};
use crate::v5::core::datasource::DataSource;
use crate::v5::core::phase::V5QueuePhase;
use crate::v5::core::queue_item::V5QueueItem;
use crate::v5::core::workspace::WorkspaceConfig;

/// v4 Gh trait를 래핑하는 v5 DataSource 구현.
///
/// GitHub API를 통해 이슈/PR을 스캔하고,
/// workspace yaml의 states.trigger.label과 매칭하여 큐 아이템을 생성한다.
pub struct GitHubDataSource {
    gh: Arc<dyn Gh>,
    source_url: String,
    /// scan 간격을 위한 마지막 스캔 시각
    last_scan: Option<chrono::DateTime<chrono::Utc>>,
}

impl GitHubDataSource {
    pub fn new(gh: Arc<dyn Gh>, source_url: &str) -> Self {
        Self {
            gh,
            source_url: source_url.to_string(),
            last_scan: None,
        }
    }

    /// URL에서 org/repo 추출
    fn extract_repo_name(url: &str) -> Option<String> {
        let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() >= 2 {
            Some(format!(
                "{}/{}",
                parts[parts.len() - 2],
                parts[parts.len() - 1]
            ))
        } else {
            None
        }
    }
}

#[async_trait]
impl DataSource for GitHubDataSource {
    fn name(&self) -> &str {
        "github"
    }

    async fn collect(&mut self, workspace: &WorkspaceConfig) -> Result<Vec<V5QueueItem>> {
        let github_config = match workspace.sources.get("github") {
            Some(config) => config,
            None => return Ok(Vec::new()),
        };

        let repo_name = Self::extract_repo_name(&github_config.url)
            .unwrap_or_else(|| "unknown/repo".to_string());

        let mut items = Vec::new();

        // 각 state의 trigger.label을 검사
        for (state_name, state_config) in &github_config.states {
            let label = match &state_config.trigger.label {
                Some(l) => l,
                None => continue,
            };

            // gh api로 해당 라벨이 붙은 이슈 목록 조회
            let open_issues = self
                .gh
                .issue_list_open(&repo_name, &format!("label:{label}"), None)
                .await;

            for issue_json in &open_issues {
                // issue_json은 "number:title" 형식 또는 JSON
                // v4의 간단한 형식을 따름
                if let Some(number) = parse_issue_number(issue_json) {
                    let source_id = format!("github:{repo_name}#{number}");
                    let work_id = V5QueueItem::make_work_id(&source_id, state_name);

                    let item = V5QueueItem {
                        work_id,
                        source_id,
                        workspace_id: workspace.name.clone(),
                        state: state_name.clone(),
                        phase: V5QueuePhase::Pending,
                        title: Some(issue_json.clone()),
                        created_at: chrono::Utc::now().to_rfc3339(),
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    };
                    items.push(item);
                }
            }
        }

        self.last_scan = Some(chrono::Utc::now());
        Ok(items)
    }

    async fn get_context(&self, item: &V5QueueItem) -> Result<ItemContext> {
        let repo_name =
            Self::extract_repo_name(&self.source_url).unwrap_or_else(|| "unknown/repo".to_string());

        // source_id에서 이슈 번호 추출: "github:org/repo#42"
        let issue_number = item
            .source_id
            .rsplit('#')
            .next()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        // gh api로 이슈 상세 조회
        let title = self
            .gh
            .api_get_field(
                &repo_name,
                &format!("issues/{issue_number}"),
                ".title",
                None,
            )
            .await
            .unwrap_or_default();

        let body = self
            .gh
            .api_get_field(&repo_name, &format!("issues/{issue_number}"), ".body", None)
            .await;

        let author = self
            .gh
            .api_get_field(
                &repo_name,
                &format!("issues/{issue_number}"),
                ".user.login",
                None,
            )
            .await
            .unwrap_or_default();

        Ok(ItemContext {
            work_id: item.work_id.clone(),
            workspace: item.workspace_id.clone(),
            queue: QueueContext {
                phase: item.phase.as_str().to_string(),
                state: item.state.clone(),
                source_id: item.source_id.clone(),
            },
            source: SourceContext {
                source_type: "github".to_string(),
                url: self.source_url.clone(),
                default_branch: Some("main".to_string()),
            },
            issue: Some(IssueContext {
                number: issue_number,
                title,
                body,
                labels: vec![],
                author,
            }),
            pr: None,
            history: vec![], // DB에서 조회 필요 — 추후 연동
            worktree: None,
        })
    }
}

/// 이슈 번호를 파싱한다.
/// gh issue list 결과는 여러 형식일 수 있으므로 첫 번째 숫자를 추출.
fn parse_issue_number(s: &str) -> Option<i64> {
    s.split_whitespace()
        .next()
        .and_then(|token| token.trim_start_matches('#').parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_repo_name_from_url() {
        assert_eq!(
            GitHubDataSource::extract_repo_name("https://github.com/org/repo"),
            Some("org/repo".to_string())
        );
        assert_eq!(
            GitHubDataSource::extract_repo_name("https://github.com/org/repo.git"),
            Some("org/repo".to_string())
        );
    }

    #[test]
    fn parse_issue_number_formats() {
        assert_eq!(parse_issue_number("42 Fix bug"), Some(42));
        assert_eq!(parse_issue_number("#42 Fix bug"), Some(42));
        assert_eq!(parse_issue_number("not a number"), None);
    }

    #[test]
    fn name_is_github() {
        use crate::infra::gh::mock::MockGh;
        let gh: Arc<dyn Gh> = Arc::new(MockGh::new());
        let ds = GitHubDataSource::new(gh, "https://github.com/org/repo");
        assert_eq!(ds.name(), "github");
    }

    #[tokio::test]
    async fn collect_no_github_source() {
        use crate::infra::gh::mock::MockGh;
        let gh: Arc<dyn Gh> = Arc::new(MockGh::new());
        let mut ds = GitHubDataSource::new(gh, "https://github.com/org/repo");
        let config: WorkspaceConfig = serde_yaml::from_str("name: test\nsources: {}").unwrap();
        let items = ds.collect(&config).await.unwrap();
        assert!(items.is_empty());
    }
}
