//! Slack channel — owns everything Slack-specific: the env var / config
//! entry shape, the human-readable text rendering, and the Incoming Webhook
//! delivery. Nothing outside this module knows Slack exists.

use crate::notify::channel::{ChannelFactory, Effects, NotifyChannel};
use crate::notify::config::ConfigEnv;
use crate::notify::event::Event;
use crate::notify::transport::HttpPoster;
use serde_json::{json, Value};

pub const ENV_WEBHOOK_URL: &str = "ATELIER_NOTIFY_SLACK_WEBHOOK_URL";

pub struct Factory;

impl ChannelFactory for Factory {
    fn kind(&self) -> &'static str {
        "slack"
    }

    fn build_from_env<'a>(
        &self,
        env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>> {
        let webhook_url = env.var(ENV_WEBHOOK_URL).filter(|s| !s.is_empty())?;
        Some(Box::new(SlackChannel {
            webhook_url,
            poster: fx.poster,
        }))
    }

    fn build_from_config<'a>(
        &self,
        entry: &Value,
        _env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>> {
        let webhook_url = entry["webhookUrl"]
            .as_str()
            .filter(|s| !s.is_empty())?
            .to_string();
        Some(Box::new(SlackChannel {
            webhook_url,
            poster: fx.poster,
        }))
    }
}

struct SlackChannel<'a> {
    webhook_url: String,
    poster: &'a dyn HttpPoster,
}

impl NotifyChannel for SlackChannel<'_> {
    fn kind(&self) -> &'static str {
        "slack"
    }

    fn send(&self, event: &Event) -> Result<(), String> {
        let body = json!({ "text": text(event) }).to_string();
        self.poster.post_json(&self.webhook_url, &body)
    }
}

/// Human-readable Slack message text for each event kind.
fn text(event: &Event) -> String {
    match event {
        Event::AskQuestion(p) => {
            let mut text = String::from(":question: *Claude 세션이 응답을 기다리고 있습니다*\n");
            if let Some(cwd) = &p.cwd {
                text.push_str(&format!("프로젝트: `{cwd}`\n"));
            }
            for q in &p.questions {
                match &q.header {
                    Some(h) => text.push_str(&format!("\n*[{h}]* {}\n", q.question)),
                    None => text.push_str(&format!("\n*{}*\n", q.question)),
                }
                for opt in &q.options {
                    text.push_str(&format!("• {opt}\n"));
                }
            }
            text
        }
        Event::Notification(p) => {
            let mut text = String::from(":bell: *Claude 세션이 입력을 기다리고 있습니다*\n");
            if let Some(cwd) = &p.cwd {
                text.push_str(&format!("프로젝트: `{cwd}`\n"));
            }
            if let Some(message) = &p.message {
                text.push_str(&format!("\n{message}\n"));
            }
            text
        }
    }
}
