//! Drift subsystem: judges whether the setup-copied artifacts still match the
//! plugin sources, and re-syncs them on request.

pub mod commands;
pub mod core;

pub use commands::cli::run_from;
