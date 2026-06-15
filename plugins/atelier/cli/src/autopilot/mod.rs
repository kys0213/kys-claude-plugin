//! Autopilot subsystem — the deterministic github-autopilot CLI absorbed into
//! the atelier crate. Internal module structure is preserved verbatim from the
//! original `autopilot` crate; only the crate-relative paths were re-rooted
//! under `crate::autopilot::*` during the consolidation move.

pub mod autopilot_md;
pub mod cmd;
pub mod config;
pub mod domain;
pub mod fs;
pub mod gh;
pub mod git;
pub mod github;
pub mod ports;
pub mod store;

pub mod run;
