//! Generic webhook channel — POSTs the event's canonical structured JSON to
//! an arbitrary URL, for receivers that format on their side (Discord relays,
//! mail bridges, custom servers).

use crate::notify::channel::{ChannelFactory, Effects, NotifyChannel};
use crate::notify::config::ConfigEnv;
use crate::notify::event::Event;
use crate::notify::transport::HttpPoster;
use serde_json::Value;

pub const ENV_URL: &str = "ATELIER_NOTIFY_WEBHOOK_URL";

pub struct Factory;

impl ChannelFactory for Factory {
    fn kind(&self) -> &'static str {
        "webhook"
    }

    fn build_from_env<'a>(
        &self,
        env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>> {
        let url = env.var(ENV_URL).filter(|s| !s.is_empty())?;
        Some(Box::new(WebhookChannel {
            url,
            poster: fx.poster,
        }))
    }

    fn build_from_config<'a>(
        &self,
        entry: &Value,
        _env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>> {
        let url = entry["url"].as_str().filter(|s| !s.is_empty())?.to_string();
        Some(Box::new(WebhookChannel {
            url,
            poster: fx.poster,
        }))
    }
}

struct WebhookChannel<'a> {
    url: String,
    poster: &'a dyn HttpPoster,
}

impl NotifyChannel for WebhookChannel<'_> {
    fn kind(&self) -> &'static str {
        "webhook"
    }

    fn send(&self, event: &Event) -> Result<(), String> {
        self.poster.post_json(&self.url, &event.structured_json())
    }
}
