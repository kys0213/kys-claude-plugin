//! Core services for the git subsystem (port of `git-utils/src/core/`).
//! Each module declares a `*Service` trait plus a real shell-backed
//! implementation so commands depend on abstractions, not the git/gh CLIs.

pub mod git;
pub mod github;
pub mod guard;
pub mod jira;
pub mod pr_guard;
pub mod shell;
