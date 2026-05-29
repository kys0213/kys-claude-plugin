//! Top-level `atelier` clap router.
//!
//! Routes to absorbed subsystems. Each subsystem owns its own clap tree, so we
//! capture its arguments verbatim and re-parse them with the subsystem's parser
//! (`Cli::parse_from`). This keeps the autopilot CLI surface byte-for-byte
//! compatible — `atelier autopilot <X>` behaves exactly like the old
//! `autopilot <X>` (Phase 2b: regression 0).
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "atelier",
    version,
    about = "통합 개발 워크플로우 CLI — autopilot / git 서브시스템 라우터"
)]
pub struct AtelierCli {
    #[command(subcommand)]
    pub command: AtelierCommand,
}

#[derive(Subcommand)]
pub enum AtelierCommand {
    /// github-autopilot deterministic CLI (replaces the `autopilot` binary)
    Autopilot {
        /// Arguments forwarded verbatim to the autopilot subsystem parser.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Parses the top-level args and dispatches to the matching subsystem, returning
/// a process exit code.
pub fn run() -> i32 {
    let cli = AtelierCli::parse();
    match cli.command {
        AtelierCommand::Autopilot { args } => {
            // Re-parse with autopilot's own clap tree. `parse_from` expects the
            // program name as argv[0]; use "autopilot" so help/error text and
            // `--version` read naturally.
            let argv = std::iter::once("autopilot".to_string()).chain(args);
            let autopilot_cli = crate::autopilot::cmd::Cli::parse_from(argv);
            crate::autopilot::run::run(autopilot_cli)
        }
    }
}
