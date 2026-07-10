//! File channel — appends the event's canonical structured JSON as one JSONL
//! line, the poll-friendly counterpart of the push channels: a Claude Code
//! Monitor (`tail -F <path>`) turns each appended line into an event. Owns
//! the `~/` home expansion its paths need.

use crate::notify::channel::{ChannelFactory, Effects, NotifyChannel};
use crate::notify::config::ConfigEnv;
use crate::notify::event::Event;
use crate::notify::transport::FileAppender;
use serde_json::Value;

pub const ENV_PATH: &str = "ATELIER_NOTIFY_FILE";

pub struct Factory;

impl ChannelFactory for Factory {
    fn kind(&self) -> &'static str {
        "file"
    }

    fn build_from_env<'a>(
        &self,
        env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>> {
        let path = env.var(ENV_PATH).filter(|s| !s.is_empty())?;
        Some(Box::new(FileChannel {
            path: expand_home(&path, env),
            appender: fx.appender,
        }))
    }

    fn build_from_config<'a>(
        &self,
        entry: &Value,
        env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>> {
        let path = entry["path"].as_str().filter(|s| !s.is_empty())?;
        Some(Box::new(FileChannel {
            path: expand_home(path, env),
            appender: fx.appender,
        }))
    }
}

/// Expands a leading `~/` with `$HOME` so a global sink path (shared across
/// sessions/projects) can be written portably. No HOME → path kept as-is.
fn expand_home(path: &str, env: &dyn ConfigEnv) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => match env.var("HOME").filter(|h| !h.is_empty()) {
            Some(home) => format!("{home}/{rest}"),
            None => path.to_string(),
        },
        None => path.to_string(),
    }
}

struct FileChannel<'a> {
    path: String,
    appender: &'a dyn FileAppender,
}

impl NotifyChannel for FileChannel<'_> {
    fn kind(&self) -> &'static str {
        "file"
    }

    fn send(&self, event: &Event) -> Result<(), String> {
        self.appender
            .append_line(&self.path, &event.structured_json())
    }
}
