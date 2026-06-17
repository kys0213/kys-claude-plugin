//! Command handlers for the git subsystem (port of `git-utils/src/commands/`).
//! Each module exposes a `run` entry that takes injected dependencies plus a
//! typed input and returns a `CmdResult`, keeping business logic out of the CLI
//! layer.

pub mod guard;
pub mod hook;
pub mod reviews;
