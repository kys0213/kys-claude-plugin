use anyhow::{bail, Result};
use async_trait::async_trait;
use serde::Serialize;

use crate::core::notifier::{NotificationEvent, Notifier};

/// Webhook URL에 JSON payload를 POST하는 Notifier.
///
/// 외부 HTTP 라이브러리 의존성을 피하기 위해
/// `curl` CLI를 `tokio::process::Command`로 호출한다.
pub struct WebhookNotifier {
    url: String,
}

/// Wraps a `NotificationEvent` with the fixed `"event": "hitl_required"` field
/// so that `serde_json::to_string` produces the complete payload.
#[derive(Serialize)]
struct WebhookPayload<'a> {
    event: &'static str,
    #[serde(flatten)]
    inner: &'a NotificationEvent,
}

impl WebhookNotifier {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    /// NotificationEvent를 JSON 문자열로 직렬화한다.
    fn build_payload(event: &NotificationEvent) -> Result<String> {
        let payload = WebhookPayload {
            event: "hitl_required",
            inner: event,
        };
        serde_json::to_string(&payload).map_err(Into::into)
    }
}

#[async_trait]
impl Notifier for WebhookNotifier {
    fn channel_name(&self) -> &str {
        "webhook"
    }

    async fn notify(&self, event: &NotificationEvent) -> Result<()> {
        let payload = Self::build_payload(event)?;

        let output = tokio::process::Command::new("curl")
            .args([
                "-s",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "--connect-timeout",
                "5",
                "--max-time",
                "10",
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "-d",
                &payload,
                &self.url,
            ])
            .output()
            .await?;

        if !output.status.success() {
            bail!(
                "curl command failed with exit code: {:?}",
                output.status.code()
            );
        }

        let status_code = String::from_utf8_lossy(&output.stdout);
        let code: u16 = status_code.trim().parse().unwrap_or(0);

        if !(200..300).contains(&code) {
            bail!("webhook returned HTTP {code}");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event() -> NotificationEvent {
        NotificationEvent {
            repo_name: "org/repo".to_string(),
            severity: "high".to_string(),
            situation: "CI failure".to_string(),
            context: "Build failed".to_string(),
            options: vec!["Retry".to_string(), "Skip".to_string()],
            work_id: Some("issue:org/repo:42".to_string()),
            spec_id: Some("spec-001".to_string()),
            url: Some("https://github.com/org/repo/issues/42".to_string()),
        }
    }

    #[test]
    fn build_payload_contains_all_fields() {
        let event = make_event();
        let payload = WebhookNotifier::build_payload(&event).unwrap();

        assert!(payload.contains("\"event\":\"hitl_required\""));
        assert!(payload.contains("\"repo_name\":\"org/repo\""));
        assert!(payload.contains("\"severity\":\"high\""));
        assert!(payload.contains("\"situation\":\"CI failure\""));
        assert!(payload.contains("\"context\":\"Build failed\""));
        assert!(payload.contains("\"options\":[\"Retry\",\"Skip\"]"));
        assert!(payload.contains("\"work_id\":\"issue:org/repo:42\""));
        assert!(payload.contains("\"spec_id\":\"spec-001\""));
        assert!(payload.contains("\"url\":\"https://github.com/org/repo/issues/42\""));
    }

    #[test]
    fn build_payload_handles_null_optionals() {
        let mut event = make_event();
        event.work_id = None;
        event.spec_id = None;
        event.url = None;

        let payload = WebhookNotifier::build_payload(&event).unwrap();

        assert!(payload.contains("\"work_id\":null"));
        assert!(payload.contains("\"spec_id\":null"));
        assert!(payload.contains("\"url\":null"));
    }

    #[test]
    fn build_payload_is_valid_json() {
        let event = make_event();
        let payload = WebhookNotifier::build_payload(&event).unwrap();
        let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(&payload);
        assert!(parsed.is_ok(), "payload is not valid JSON: {payload}");
    }

    #[test]
    fn build_payload_escapes_quotes() {
        let mut event = make_event();
        event.situation = "found \"error\" in log".to_string();
        let payload = WebhookNotifier::build_payload(&event).unwrap();
        let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(&payload);
        assert!(
            parsed.is_ok(),
            "payload with quotes is not valid JSON: {payload}"
        );
    }

    #[test]
    fn channel_name_is_webhook() {
        let notifier = WebhookNotifier::new("http://example.com".to_string());
        assert_eq!(notifier.channel_name(), "webhook");
    }
}
