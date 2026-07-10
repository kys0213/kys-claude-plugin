//! Desktop channel — OS notification banner on the same machine (osascript /
//! notify-send). Catches "working in another window" without network or
//! secrets. Owns the banner (title, body) rendering: banners truncate, so
//! only the first question + a count of the rest.

use crate::notify::channel::{ChannelFactory, Effects, NotifyChannel};
use crate::notify::config::ConfigEnv;
use crate::notify::event::Event;
use crate::notify::transport::DesktopNotifier;
use serde_json::Value;

pub const ENV_DESKTOP: &str = "ATELIER_NOTIFY_DESKTOP";

pub struct Factory;

impl ChannelFactory for Factory {
    fn kind(&self) -> &'static str {
        "desktop"
    }

    fn build_from_env<'a>(
        &self,
        env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>> {
        if !env.var(ENV_DESKTOP).map(|v| truthy(&v)).unwrap_or(false) {
            return None;
        }
        Some(Box::new(DesktopChannel {
            notifier: fx.desktop,
        }))
    }

    fn build_from_config<'a>(
        &self,
        _entry: &Value,
        _env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>> {
        Some(Box::new(DesktopChannel {
            notifier: fx.desktop,
        }))
    }
}

fn truthy(v: &str) -> bool {
    !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
}

struct DesktopChannel<'a> {
    notifier: &'a dyn DesktopNotifier,
}

impl NotifyChannel for DesktopChannel<'_> {
    fn kind(&self) -> &'static str {
        "desktop"
    }

    fn send(&self, event: &Event) -> Result<(), String> {
        let (title, body) = banner(event);
        self.notifier.notify(&title, &body)
    }
}

/// Banner (title, body) for each event kind.
fn banner(event: &Event) -> (String, String) {
    match event {
        Event::AskQuestion(p) => {
            let mut body = match p.questions.first() {
                Some(q) => q.question.clone(),
                None => String::new(),
            };
            if p.questions.len() > 1 {
                body.push_str(&format!(" (외 {}개)", p.questions.len() - 1));
            }
            if let Some(cwd) = &p.cwd {
                body.push_str(&format!("\n{cwd}"));
            }
            ("Claude 질문 대기".to_string(), body)
        }
        Event::Notification(p) => {
            let mut body = p.message.clone().unwrap_or_default();
            if let Some(cwd) = &p.cwd {
                body.push_str(&format!("\n{cwd}"));
            }
            ("Claude 입력 대기".to_string(), body)
        }
    }
}
