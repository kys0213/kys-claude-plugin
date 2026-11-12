//! Orchestrator subsystem — deterministic dispatch-rule checks behind the
//! function hooks module; the CLI surface lives in [`commands`].

pub mod commands;
pub mod core;

pub use commands::{run, run_from, Cli, Commands};
