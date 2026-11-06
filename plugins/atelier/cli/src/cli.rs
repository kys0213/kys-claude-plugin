//! Top-level atelier clap router. Dispatches `atelier <subsystem> <args...>` by
//! re-parsing the trailing args with each subsystem's own clap surface, so each
//! subsystem owns its argument grammar, `--help` and `--version`.

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "atelier",
    version,
    about = "Unified development workflow CLI (drift, code-style, git, session, orchestrator, kanban)"
)]
pub struct AtelierCli {
    #[command(subcommand)]
    pub command: AtelierCommand,
}

#[derive(clap::Subcommand)]
pub enum AtelierCommand {
    /// Setup-copied artifact drift judgement and re-sync
    #[command(disable_help_flag = true)]
    Drift {
        /// Arguments forwarded verbatim to the drift subsystem
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Project-scope code-style rule install, update and drift check
    #[command(disable_help_flag = true)]
    CodeStyle {
        /// Arguments forwarded verbatim to the code-style subsystem
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
    /// Session-scoped hook helpers (push-check / ensure-env)
    #[command(disable_help_flag = true)]
    Session {
        /// Arguments forwarded verbatim to the session subsystem
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Orchestrator dispatch-rule checks for function hooks (spawn-check / compact-note)
    #[command(disable_help_flag = true)]
    Orchestrator {
        /// Arguments forwarded verbatim to the orchestrator subsystem
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Session kanban board for the hooks module's pane
    #[command(disable_help_flag = true)]
    Kanban {
        /// Arguments forwarded verbatim to the kanban subsystem
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Returns the process exit code of the selected subsystem.
pub fn run() -> i32 {
    let cli = AtelierCli::parse();
    match cli.command {
        AtelierCommand::Drift { args } => {
            let argv = std::iter::once("drift".to_string()).chain(args);
            crate::drift::run_from(argv)
        }
        AtelierCommand::CodeStyle { args } => {
            let argv = std::iter::once("code-style".to_string()).chain(args);
            crate::code_style::run_from(argv)
        }
        AtelierCommand::Git { args } => {
            let argv = std::iter::once("git".to_string()).chain(args);
            crate::git::run_from(argv)
        }
        AtelierCommand::Session { args } => {
            let argv = std::iter::once("session".to_string()).chain(args);
            crate::session::run_from(argv)
        }
        AtelierCommand::Orchestrator { args } => {
            let argv = std::iter::once("orchestrator".to_string()).chain(args);
            crate::orchestrator::run_from(argv)
        }
        AtelierCommand::Kanban { args } => {
            let argv = std::iter::once("kanban".to_string()).chain(args);
            crate::kanban::run_from(argv)
        }
    }
}
