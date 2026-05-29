//! Top-level atelier clap router. Dispatches `atelier <subsystem> <args...>` to
//! the absorbed subsystems by re-parsing the trailing args with each
//! subsystem's own clap surface. This keeps the subsystems' argument grammars
//! independent and unchanged from their standalone CLIs.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "atelier",
    version,
    about = "Unified development workflow CLI (git, autopilot, ...)"
)]
pub struct AtelierCli {
    #[command(subcommand)]
    pub command: AtelierCommand,
}

#[derive(clap::Subcommand)]
pub enum AtelierCommand {
    /// github-autopilot deterministic CLI
    #[command(disable_help_flag = true)]
    Autopilot {
        /// Arguments forwarded verbatim to the autopilot subsystem
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// git-utils workflow automation CLI
    #[command(disable_help_flag = true)]
    Git {
        /// Arguments forwarded verbatim to the git subsystem
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Parses argv and dispatches to the selected subsystem, returning a process
/// exit code. Each subsystem re-parses its own args so its grammar (and
/// `--help`/`--version`) behaves exactly as the standalone CLI did.
pub fn run() -> i32 {
    let cli = AtelierCli::parse();
    match cli.command {
        AtelierCommand::Autopilot { args } => {
            // Re-prepend the binary name so clap's argv[0] expectation holds.
            let argv = std::iter::once("autopilot".to_string()).chain(args);
            let autopilot_cli = crate::autopilot::cmd::Cli::parse_from(argv);
            crate::autopilot::run::run(autopilot_cli)
        }
        AtelierCommand::Git { args } => {
            let argv = std::iter::once("git".to_string()).chain(args);
            crate::git::run_from(argv)
        }
    }
}
