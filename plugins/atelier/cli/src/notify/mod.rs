//! Notify subsystem — routes Claude Code hook events (`AskUserQuestion`
//! PreToolUse, Notification) to configured message channels (Slack webhook /
//! generic webhook). Runs as advisory hooks, so the exit-code contract
//! differs from the git subsystem: **every path exits 0** — exit 2 would
//! block the observed tool, and even a usage error must stay advisory.
//! Failures are reported in the JSON output instead.

pub mod command;
pub mod config;
pub mod message;
pub mod payload;
pub mod transport;
pub mod types;

use crate::notify::config::{resolve_channels, ConfigEnv, ConfigFs};
use crate::notify::transport::{CurlPoster, RealDesktopNotifier, RealFileAppender};
use crate::notify::types::{AskQuestionPayload, NotificationPayload, NotifyOutput};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "notify",
    version,
    about = "Channel notifications for Claude Code hooks"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Forward an AskUserQuestion PreToolUse payload (stdin) to configured channels
    #[command(name = "ask-question")]
    AskQuestion {
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
    },
    /// Forward a Notification hook payload (stdin) — permission requests, idle waits
    #[command(name = "notification")]
    Notification {
        #[arg(long = "project-dir")]
        project_dir: Option<String>,
    },
}

struct RealEnv;

impl ConfigEnv for RealEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

struct RealFs;

impl ConfigFs for RealFs {
    fn read_file(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }
}

/// Reads stdin to a string (empty on read failure) — same edge-only I/O rule
/// as the git subsystem (#778).
fn read_stdin_raw() -> String {
    use std::io::Read as _;
    let mut buf = String::new();
    let _ = std::io::stdin().read_to_string(&mut buf);
    buf
}

/// Shared edge wiring: resolve channels for the project, run the event's
/// command core over stdin, print the JSON report. Always 0.
fn dispatch(
    project_dir: Option<String>,
    run: impl FnOnce(&command::NotifyDeps, &[types::Channel], &str) -> NotifyOutput,
) -> i32 {
    let project_dir = project_dir.unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    });
    let channels = resolve_channels(&RealEnv, &RealFs, &project_dir);
    let raw = read_stdin_raw();
    let deps = command::NotifyDeps {
        poster: &CurlPoster,
        appender: &RealFileAppender,
        desktop: &RealDesktopNotifier,
    };
    let output = run(&deps, &channels, &raw);
    let json = serde_json::to_string_pretty(&output).unwrap_or_else(|_| "null".to_string());
    println!("{json}");
    0
}

/// Parses `argv` and runs the selected command. Always returns 0: parse
/// errors (including `--help`) print and no-op so a misregistered hook can
/// never block the tool call it observes.
pub fn run_from<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = match Cli::try_parse_from(argv) {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return 0;
        }
    };

    match cli.command {
        Some(Commands::AskQuestion { project_dir }) => dispatch(project_dir, |deps, ch, raw| {
            command::run_ask_question(deps, ch, &AskQuestionPayload::parse(raw))
        }),
        Some(Commands::Notification { project_dir }) => dispatch(project_dir, |deps, ch, raw| {
            command::run_notification(deps, ch, &NotificationPayload::parse(raw))
        }),
        None => {
            use clap::CommandFactory;
            let _ = Cli::command().print_help();
            println!();
            0
        }
    }
}
