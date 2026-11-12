//! Session subsystem — helpers behind the plugin's session-boundary hooks;
//! the CLI surface lives in [`commands`].

pub mod commands;
pub mod core;

pub use commands::run_from;
