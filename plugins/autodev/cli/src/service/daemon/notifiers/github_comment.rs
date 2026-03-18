use std::sync::Arc;

use anyhow::{bail, Result};
use async_trait::async_trait;

use crate::core::notifier::{NotificationEvent, Notifier};
use crate::infra::gh::Gh;

/// GitHub issue/PR에 마크다운 코멘트로 HITL 알림을 게시하는 Notifier.
///
/// `work_id` 형식: `"issue:org/repo:42"` 또는 `"pr:org/repo:15"`
/// 에서 issue/PR 번호를 추출하여 해당 issue/PR에 코멘트를 남긴다.
pub struct GitHubCommentNotifier {
    gh: Arc<dyn Gh>,
    gh_host: Option<String>,
}

impl GitHubCommentNotifier {
    pub fn new(gh: Arc<dyn Gh>, gh_host: Option<String>) -> Self {
        Self { gh, gh_host }
    }

    /// work_id에서 (repo_name, number)를 추출한다.
    /// 형식: "issue:org/repo:42" 또는 "pr:org/repo:15"
    fn parse_work_id(work_id: &str) -> Option<(String, i64)> {
        let parts: Vec<&str> = work_id.splitn(3, ':').collect();
        if parts.len() != 3 {
            return None;
        }
        let repo_name = parts[1].to_string();
        let number = parts[2].parse::<i64>().ok()?;
        Some((repo_name, number))
    }

    /// 알림 마크다운 본문을 생성한다.
    ///
    /// HITL ID가 있으면 HTML 마커를 포함하여 reply-scanning이 가능하게 한다.
    fn format_comment(event: &NotificationEvent) -> String {
        let mut body = String::new();

        // HITL reply-scanning marker (invisible in rendered markdown)
        if let Some(ref hitl_id) = event.hitl_id {
            body.push_str(&format!("<!-- autodev:hitl:{hitl_id} -->\n"));
        }

        body.push_str("## \u{1f514} autodev: 사람 확인 필요\n\n");
        body.push_str(&format!("**상황**: {}\n\n", event.situation));
        body.push_str(&format!("**분석**: {}\n\n", event.context));

        if !event.options.is_empty() {
            body.push_str("**선택지**:\n");
            for (i, opt) in event.options.iter().enumerate() {
                body.push_str(&format!("{}. {}\n", i + 1, opt));
            }
            body.push_str("\n> 이 코멘트에 선택 번호(예: `1`)로 답글을 달면 자동 응답됩니다.\n");
        }

        body
    }
}

#[async_trait]
impl Notifier for GitHubCommentNotifier {
    fn channel_name(&self) -> &str {
        "github_comment"
    }

    async fn notify(&self, event: &NotificationEvent) -> Result<()> {
        let work_id = match &event.work_id {
            Some(id) => id,
            None => bail!("work_id is required for GitHub comment notification"),
        };

        let (repo_name, number) = match Self::parse_work_id(work_id) {
            Some(parsed) => parsed,
            None => bail!("invalid work_id format: {work_id}"),
        };

        let body = Self::format_comment(event);
        let host = self.gh_host.as_deref();

        if !self.gh.issue_comment(&repo_name, number, &body, host).await {
            bail!("failed to post GitHub comment on {repo_name}#{number}");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::gh::mock::MockGh;

    fn make_event(work_id: Option<&str>) -> NotificationEvent {
        NotificationEvent {
            repo_name: "org/repo".to_string(),
            severity: "high".to_string(),
            situation: "CI failure detected".to_string(),
            context: "Build failed on main branch".to_string(),
            options: vec!["Retry build".to_string(), "Skip and continue".to_string()],
            work_id: work_id.map(|s| s.to_string()),
            spec_id: None,
            url: None,
            hitl_id: Some("hitl-test-001".to_string()),
        }
    }

    #[test]
    fn parse_work_id_issue_format() {
        let result = GitHubCommentNotifier::parse_work_id("issue:org/repo:42");
        assert_eq!(result, Some(("org/repo".to_string(), 42)));
    }

    #[test]
    fn parse_work_id_pr_format() {
        let result = GitHubCommentNotifier::parse_work_id("pr:org/repo:15");
        assert_eq!(result, Some(("org/repo".to_string(), 15)));
    }

    #[test]
    fn parse_work_id_invalid_format() {
        assert!(GitHubCommentNotifier::parse_work_id("invalid").is_none());
        assert!(GitHubCommentNotifier::parse_work_id("issue:repo").is_none());
        assert!(GitHubCommentNotifier::parse_work_id("issue:org/repo:abc").is_none());
    }

    #[test]
    fn format_comment_includes_all_fields() {
        let event = make_event(Some("issue:org/repo:42"));
        let body = GitHubCommentNotifier::format_comment(&event);

        assert!(body.contains("사람 확인 필요"));
        assert!(body.contains("CI failure detected"));
        assert!(body.contains("Build failed on main branch"));
        assert!(body.contains("1. Retry build"));
        assert!(body.contains("2. Skip and continue"));
    }

    #[test]
    fn format_comment_empty_options() {
        let mut event = make_event(Some("issue:org/repo:42"));
        event.options = vec![];
        let body = GitHubCommentNotifier::format_comment(&event);

        assert!(body.contains("CI failure detected"));
        assert!(!body.contains("선택지"));
    }

    #[tokio::test]
    async fn notify_posts_comment_to_github() {
        let mock_gh = Arc::new(MockGh::new());
        let gh: Arc<dyn Gh> = Arc::clone(&mock_gh) as Arc<dyn Gh>;
        let notifier = GitHubCommentNotifier::new(gh, None);

        let event = make_event(Some("issue:org/repo:42"));
        let result = notifier.notify(&event).await;

        assert!(result.is_ok());

        let comments = mock_gh.posted_comments.lock().unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].0, "org/repo");
        assert_eq!(comments[0].1, 42);
        assert!(comments[0].2.contains("CI failure detected"));
    }

    #[tokio::test]
    async fn notify_fails_when_work_id_missing() {
        let gh: Arc<dyn Gh> = Arc::new(MockGh::new());
        let notifier = GitHubCommentNotifier::new(gh, None);

        let event = make_event(None);
        let result = notifier.notify(&event).await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("work_id is required"));
    }

    #[tokio::test]
    async fn notify_fails_when_work_id_invalid() {
        let gh: Arc<dyn Gh> = Arc::new(MockGh::new());
        let notifier = GitHubCommentNotifier::new(gh, None);

        let event = make_event(Some("bad-format"));
        let result = notifier.notify(&event).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid work_id"));
    }
}
