//! Git subsystem — Rust port of the `git-utils` TypeScript CLI.

pub mod commands;
pub mod core;
pub mod types;

pub use commands::cli::{run, run_from, Cli, Commands, SetupCommand};
