//! Channel abstraction — the OCP seam of the notify subsystem.
//!
//! One channel = one module owning its whole vertical (SRP): how its config
//! entry / env var is parsed, how an `Event` is rendered for its medium, and
//! how the rendered message is sent (through the pure-I/O ports in
//! `transport`). Everything outside `channel/` is channel-agnostic:
//! `config` knows only resolution precedence, `command` only gates + fan-out.
//!
//! Adding a channel (e.g. email) = one new module implementing
//! `NotifyChannel` + `ChannelFactory`, plus one line in `registry()`. No
//! existing code changes — the registry list is the single, acknowledged
//! modification point.

pub mod desktop;
pub mod file;
pub mod slack;
pub mod webhook;

use crate::notify::config::ConfigEnv;
use crate::notify::event::Event;
use crate::notify::transport::{DesktopNotifier, FileAppender, HttpPoster};
use serde_json::Value;

/// Pure-I/O ports channels send through. Injected once at the edge (real
/// impls) or in tests (stubs); channels pick what they need.
pub struct Effects<'a> {
    pub poster: &'a dyn HttpPoster,
    pub appender: &'a dyn FileAppender,
    pub desktop: &'a dyn DesktopNotifier,
}

/// A resolved delivery channel: renders the event for its medium and sends.
pub trait NotifyChannel {
    /// Stable kind name used in reports and as the config `type` tag.
    fn kind(&self) -> &'static str;
    fn send(&self, event: &Event) -> Result<(), String>;
}

/// Constructs a channel from its env var(s) or its config-file entry. `None`
/// means "not configured here" — resolution just moves on.
pub trait ChannelFactory {
    /// The config-file `type` tag this factory answers to.
    fn kind(&self) -> &'static str;
    fn build_from_env<'a>(
        &self,
        env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>>;
    fn build_from_config<'a>(
        &self,
        entry: &Value,
        env: &dyn ConfigEnv,
        fx: &Effects<'a>,
    ) -> Option<Box<dyn NotifyChannel + 'a>>;
}

/// Every known channel factory, in env-resolution order. New channels
/// register here — this list is the only place that names them all.
pub fn registry() -> Vec<Box<dyn ChannelFactory>> {
    vec![
        Box::new(slack::Factory),
        Box::new(webhook::Factory),
        Box::new(file::Factory),
        Box::new(desktop::Factory),
    ]
}
