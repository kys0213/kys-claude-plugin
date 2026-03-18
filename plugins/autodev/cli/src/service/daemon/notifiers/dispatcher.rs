use std::sync::Arc;

use crate::core::config::models::DaemonConfig;
use crate::core::notifier::{NotificationEvent, Notifier};
use crate::infra::gh::Gh;

use super::github_comment::GitHubCommentNotifier;
use super::webhook::WebhookNotifier;

/// 등록된 모든 Notifier에게 순차 전송. 개별 채널 실패 시 로그만 남기고 계속.
pub struct NotificationDispatcher {
    notifiers: Vec<Box<dyn Notifier>>,
}

impl NotificationDispatcher {
    pub fn new(notifiers: Vec<Box<dyn Notifier>>) -> Self {
        Self { notifiers }
    }

    /// Build a dispatcher from daemon config. Returns `None` if no channels are configured.
    ///
    /// Supports both legacy `webhook_url` and new `notifications.channels` config.
    /// If `gh` is provided, `github_comment` channels are wired.
    pub fn from_config(cfg: &DaemonConfig) -> Option<Self> {
        Self::from_config_with_gh(cfg, None, None)
    }

    /// Build a dispatcher with optional GitHub client for comment notifications.
    pub fn from_config_with_gh(
        cfg: &DaemonConfig,
        gh: Option<Arc<dyn Gh>>,
        gh_host: Option<String>,
    ) -> Option<Self> {
        let mut notifiers: Vec<Box<dyn Notifier>> = Vec::new();

        // Legacy: daemon.webhook_url
        if let Some(ref url) = cfg.webhook_url {
            notifiers.push(Box::new(WebhookNotifier::new(url.clone())));
        }

        // New: notifications.channels
        for channel in &cfg.notifications.channels {
            match channel.channel_type.as_str() {
                "webhook" => {
                    if let Some(ref url) = channel.config.url {
                        notifiers.push(Box::new(WebhookNotifier::new(url.clone())));
                    }
                }
                "github_comment" => {
                    if let Some(ref gh_client) = gh {
                        notifiers.push(Box::new(GitHubCommentNotifier::new(
                            Arc::clone(gh_client),
                            gh_host.clone(),
                        )));
                    }
                }
                other => {
                    tracing::warn!("unknown notification channel type: {other}");
                }
            }
        }

        if notifiers.is_empty() {
            None
        } else {
            Some(Self::new(notifiers))
        }
    }

    /// 모든 notifier에 이벤트를 전송하고, 실패한 채널의 (이름, 에러) 목록을 반환.
    pub async fn dispatch(&self, event: &NotificationEvent) -> Vec<(String, anyhow::Error)> {
        let mut errors = Vec::new();
        for notifier in &self.notifiers {
            if let Err(e) = notifier.notify(event).await {
                tracing::warn!("notifier '{}' failed: {e}", notifier.channel_name());
                errors.push((notifier.channel_name().to_string(), e));
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// 성공하는 mock notifier
    struct SuccessNotifier {
        name: String,
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Notifier for SuccessNotifier {
        fn channel_name(&self) -> &str {
            &self.name
        }

        async fn notify(&self, event: &NotificationEvent) -> anyhow::Result<()> {
            self.calls.lock().unwrap().push(event.situation.clone());
            Ok(())
        }
    }

    /// 실패하는 mock notifier
    struct FailNotifier {
        name: String,
    }

    #[async_trait]
    impl Notifier for FailNotifier {
        fn channel_name(&self) -> &str {
            &self.name
        }

        async fn notify(&self, _event: &NotificationEvent) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("channel down"))
        }
    }

    fn make_event() -> NotificationEvent {
        NotificationEvent {
            repo_name: "org/repo".to_string(),
            severity: "high".to_string(),
            situation: "test situation".to_string(),
            context: "test context".to_string(),
            options: vec!["opt1".to_string(), "opt2".to_string()],
            work_id: Some("issue:org/repo:42".to_string()),
            spec_id: None,
            url: None,
            hitl_id: None,
        }
    }

    #[tokio::test]
    async fn dispatch_sends_to_all_channels() {
        let calls_a = Arc::new(Mutex::new(Vec::new()));
        let calls_b = Arc::new(Mutex::new(Vec::new()));

        let dispatcher = NotificationDispatcher::new(vec![
            Box::new(SuccessNotifier {
                name: "a".to_string(),
                calls: Arc::clone(&calls_a),
            }),
            Box::new(SuccessNotifier {
                name: "b".to_string(),
                calls: Arc::clone(&calls_b),
            }),
        ]);

        let errors = dispatcher.dispatch(&make_event()).await;

        assert!(errors.is_empty());
        assert_eq!(calls_a.lock().unwrap().len(), 1);
        assert_eq!(calls_b.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn dispatch_collects_errors_from_failed_channels() {
        let calls = Arc::new(Mutex::new(Vec::new()));

        let dispatcher = NotificationDispatcher::new(vec![
            Box::new(SuccessNotifier {
                name: "ok".to_string(),
                calls: Arc::clone(&calls),
            }),
            Box::new(FailNotifier {
                name: "broken".to_string(),
            }),
        ]);

        let errors = dispatcher.dispatch(&make_event()).await;

        // success notifier still got called
        assert_eq!(calls.lock().unwrap().len(), 1);
        // one failure collected
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "broken");
    }

    #[tokio::test]
    async fn dispatch_with_empty_notifiers_returns_no_errors() {
        let dispatcher = NotificationDispatcher::new(vec![]);
        let errors = dispatcher.dispatch(&make_event()).await;
        assert!(errors.is_empty());
    }
}
