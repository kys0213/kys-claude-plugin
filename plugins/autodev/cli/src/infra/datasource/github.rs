//! GitHubDataSource — GitHub 이슈/PR 스캔 기반 DataSource 구현체.
//!
//! GitHub API를 통해 라벨 기반 trigger에 매칭되는 아이템을 수집하고,
//! 이슈/PR의 컨텍스트를 조회한다.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::core::datasource::{
    DataSource, ItemContext, QueueContext, SourceContext, WorkspaceConfig,
};
use crate::core::models::RepoIssue;
use crate::core::phase::TaskKind;
use crate::core::queue_item::{QueueItem, RepoRef};
use crate::infra::gh::Gh;

/// GitHub 이슈/PR 기반 DataSource.
///
/// workspace yaml의 sources.github 설정에 따라
/// 라벨 기반 trigger에 매칭되는 이슈를 스캔한다.
pub struct GitHubDataSource {
    gh: Arc<dyn Gh>,
}

impl GitHubDataSource {
    pub fn new(gh: Arc<dyn Gh>) -> Self {
        Self { gh }
    }

    /// repo URL에서 "org/repo" 형식의 이름을 추출한다.
    fn repo_name_from_url(url: &str) -> Option<String> {
        let url = url.trim_end_matches('/');
        let parts: Vec<&str> = url.rsplitn(3, '/').collect();
        if parts.len() >= 2 {
            Some(format!("{}/{}", parts[1], parts[0]))
        } else {
            None
        }
    }

    /// GitHub 이슈 목록에서 autodev 라벨이 있는 이슈를 QueueItem으로 변환한다.
    fn issues_to_queue_items(issues: &[RepoIssue], repo_ref: &RepoRef) -> Vec<QueueItem> {
        issues
            .iter()
            .filter_map(|issue| {
                let task_kind = Self::task_kind_from_labels(&issue.labels)?;
                Some(QueueItem::from_issue(repo_ref, issue, task_kind))
            })
            .collect()
    }

    /// 라벨에서 TaskKind를 결정한다.
    fn task_kind_from_labels(labels: &[String]) -> Option<TaskKind> {
        for label in labels {
            match label.as_str() {
                "autodev:analyze" => return Some(TaskKind::Analyze),
                "autodev:implement" => return Some(TaskKind::Implement),
                "autodev:review" => return Some(TaskKind::Review),
                "autodev:improve" => return Some(TaskKind::Improve),
                "autodev:extract" => return Some(TaskKind::Extract),
                _ => {}
            }
        }
        None
    }
}

#[async_trait]
impl DataSource for GitHubDataSource {
    fn name(&self) -> &str {
        "github"
    }

    async fn collect(&self, workspace: &WorkspaceConfig) -> Result<Vec<QueueItem>> {
        let source_config = workspace
            .sources
            .get("github")
            .context("github source not configured in workspace")?;

        let repo_name = Self::repo_name_from_url(&source_config.url)
            .context("failed to parse repo name from URL")?;

        let repo_ref = RepoRef {
            id: repo_name.clone(),
            name: repo_name.clone(),
            url: source_config.url.clone(),
            gh_host: None,
        };

        // Fetch open issues with pagination
        let raw = self
            .gh
            .api_paginate(
                &repo_name,
                "issues",
                &[("state", "open"), ("sort", "updated")],
                None,
            )
            .await
            .context("failed to fetch issues from GitHub")?;

        let json: serde_json::Value =
            serde_json::from_slice(&raw).context("failed to parse GitHub issues response")?;

        let issues: Vec<RepoIssue> = json
            .as_array()
            .map(|arr| arr.iter().filter_map(RepoIssue::from_json).collect())
            .unwrap_or_default();

        Ok(Self::issues_to_queue_items(&issues, &repo_ref))
    }

    async fn get_context(&self, item: &QueueItem) -> Result<ItemContext> {
        let source_id = format!("github:{}#{}", item.repo_name, item.github_number);

        // Fetch issue details for context
        let mut extra = HashMap::new();

        let issue_json = serde_json::json!({
            "number": item.github_number,
            "title": item.title,
        });
        extra.insert("issue".to_string(), issue_json);

        Ok(ItemContext {
            work_id: item.work_id.clone(),
            workspace: String::new(), // filled by caller
            queue: QueueContext {
                phase: "Running".to_string(),
                state: item.task_kind.as_str().to_string(),
                source_id,
            },
            source: SourceContext {
                source_type: "github".to_string(),
                url: item.repo_url.clone(),
                default_branch: Some("main".to_string()),
                extra,
            },
            history: vec![],
            worktree: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_name_from_url_parses_correctly() {
        assert_eq!(
            GitHubDataSource::repo_name_from_url("https://github.com/org/repo"),
            Some("org/repo".to_string())
        );
        assert_eq!(
            GitHubDataSource::repo_name_from_url("https://github.com/org/repo/"),
            Some("org/repo".to_string())
        );
    }

    #[test]
    fn task_kind_from_labels_matches_autodev_labels() {
        assert_eq!(
            GitHubDataSource::task_kind_from_labels(&["autodev:analyze".to_string()]),
            Some(TaskKind::Analyze)
        );
        assert_eq!(
            GitHubDataSource::task_kind_from_labels(&["autodev:implement".to_string()]),
            Some(TaskKind::Implement)
        );
        assert_eq!(
            GitHubDataSource::task_kind_from_labels(&["bug".to_string()]),
            None
        );
    }

    #[test]
    fn issues_to_queue_items_filters_by_autodev_labels() {
        let repo_ref = RepoRef {
            id: "org/repo".into(),
            name: "org/repo".into(),
            url: "https://github.com/org/repo".into(),
            gh_host: None,
        };

        let issues = vec![
            RepoIssue {
                number: 1,
                title: "Issue with autodev label".into(),
                body: Some("body".into()),
                author: "user".into(),
                labels: vec!["autodev:analyze".into()],
            },
            RepoIssue {
                number: 2,
                title: "Regular issue".into(),
                body: None,
                author: "user".into(),
                labels: vec!["bug".into()],
            },
        ];

        let items = GitHubDataSource::issues_to_queue_items(&issues, &repo_ref);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].github_number, 1);
        assert_eq!(items[0].task_kind, TaskKind::Analyze);
    }
}
