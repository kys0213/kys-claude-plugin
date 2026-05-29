//! atelier — unified development workflow CLI.
//!
//! A single binary that routes to absorbed subsystems via subcommands:
//!   - `atelier autopilot <...>` — the github-autopilot deterministic CLI
//!   - `atelier git <...>` — git-utils (ported to Rust, Phase 2c–2f)
//!
//! Each subsystem keeps its own clap tree under its module; the top-level
//! [`cli`] router dispatches to them.
pub mod autopilot;
pub mod cli;
