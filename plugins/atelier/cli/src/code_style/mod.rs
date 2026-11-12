//! Code-style subsystem: installs the plugin's base, per-language and
//! per-app-kind rule files into a project's `.claude/rules`, judges the
//! installed copies against the sources, and brings them back in line.

pub mod commands;
pub mod core;

pub use commands::cli::{run, run_from, Cli, Commands, Location, StyleArgs};
