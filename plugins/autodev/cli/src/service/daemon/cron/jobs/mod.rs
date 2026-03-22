//! Built-in cron job handlers.
//!
//! Each module provides Rust-native logic for a specific built-in cron job.
//! These are invoked alongside shell script execution to provide deterministic
//! operations that don't require LLM calls.

pub mod gap_detection;
pub mod knowledge_extract;
pub mod log_cleanup;
