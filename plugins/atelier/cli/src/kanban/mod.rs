//! Kanban subsystem — turns the session facts the hooks module keeps in
//! `$.store` into the board its pane draws, and carries requests between
//! sessions through their inboxes.

pub mod commands;
pub mod core;

pub use commands::run_from;
