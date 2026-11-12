//! Command handlers for the git subsystem (port of `git-utils/src/commands/`).
//! `cli` parses the arguments and wires the real services into the other
//! modules, which take their dependencies injected.

pub mod cli;
pub mod guard;
pub mod guard_setup;
pub mod hook;
pub mod reviews;
