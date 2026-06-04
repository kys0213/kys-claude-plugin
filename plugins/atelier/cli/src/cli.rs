use clap::{Parser, Subcommand};

/// Top-level `atelier` CLI router. Each domain (autopilot, git, spec, …) is a
/// subcommand group; this crate currently routes the `autopilot` group, with
/// further groups folded in as the consolidation progresses.
#[derive(Parser)]
#[command(name = "atelier", version, about = "Unified development workflow CLI")]
pub struct Atelier {
    #[command(subcommand)]
    pub command: Group,
}

#[derive(Subcommand)]
pub enum Group {
    /// GitHub autopilot — autonomous dev loop (gap detect, implement, merge, CI)
    Autopilot(crate::autopilot::cmd::AutopilotCli),
}
