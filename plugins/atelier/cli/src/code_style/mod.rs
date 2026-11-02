//! Code-style subsystem: installs the plugin's per-language rule files (plus
//! the shared `common` group, rendered for the installed languages) into a
//! project's `.claude/rules`, judges the installed copies against what they
//! should hold, and brings them back in line.
//!
//! Errors exit 2, like `drift`. Output is human text parsed line by line —
//! every rendered line format is a contract.

pub mod commands;
pub mod core;

use crate::code_style::commands::CodeStyleDeps;
use crate::code_style::core::types::CodeStylePaths;
use crate::drift::core::artifact::{create_artifact_fs, create_backup_clock};
use crate::shared::process::default_project_dir;
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "code-style",
    version,
    about = "Install, update and check project-scope code-style rules"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Install the given languages' rules and the common rules rendered for
    /// every installed language
    Install {
        #[command(flatten)]
        args: StyleArgs,
    },
    /// Back up and overwrite drifted copies, install new rules, back up and
    /// remove orphaned copies; excluded rules stay out
    Update {
        #[command(flatten)]
        args: StyleArgs,
    },
    /// Report per-rule status of the installed copies (read-only)
    Check {
        #[command(flatten)]
        args: StyleArgs,
    },
    /// Install excluded rules (copies the project deleted) again
    Restore {
        #[command(flatten)]
        location: Location,
        /// Rule as <group>/<file>.md (e.g. common/comments.md); repeatable
        #[arg(long = "rule")]
        rules: Vec<String>,
    },
    /// Back up and remove the given languages' rules and re-render the common
    /// rules for the languages left
    Uninstall {
        #[command(flatten)]
        args: StyleArgs,
    },
}

#[derive(Args)]
pub struct Location {
    /// Plugin root holding rules/code-style/<group>/
    #[arg(long = "plugin-root")]
    plugin_root: String,
    /// Project whose .claude/rules/atelier/code-style is managed (default: cwd)
    #[arg(long = "project-dir")]
    project_dir: Option<String>,
}

impl Location {
    fn paths(self) -> CodeStylePaths {
        CodeStylePaths {
            plugin_root: self.plugin_root,
            project_dir: default_project_dir(self.project_dir),
        }
    }
}

#[derive(Args)]
pub struct StyleArgs {
    #[command(flatten)]
    location: Location,
    /// Language directory under rules/code-style (e.g. go, rust); repeatable.
    /// Required by install and uninstall; update and check default to every
    /// language installed in the project
    #[arg(long = "lang")]
    langs: Vec<String>,
}

impl StyleArgs {
    fn split(self) -> (CodeStylePaths, Vec<String>) {
        (self.location.paths(), self.langs)
    }
}

fn fail(message: &str) -> i32 {
    eprintln!("Error: {message}");
    2
}

/// `argv` includes the leading program name.
pub fn run_from<I, T>(argv: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    run(Cli::parse_from(argv))
}

pub fn run(cli: Cli) -> i32 {
    let command = match cli.command {
        Some(c) => c,
        None => {
            use clap::CommandFactory;
            let _ = Cli::command().print_help();
            println!();
            return 0;
        }
    };

    let fs = create_artifact_fs();
    let clock = create_backup_clock();
    let deps = CodeStyleDeps {
        fs: &fs,
        clock: &clock,
    };
    let result = match command {
        Commands::Install { args } => {
            let (paths, langs) = args.split();
            commands::install::run(&deps, &paths, &langs).map(|r| (r.render(), 0))
        }
        Commands::Update { args } => {
            let (paths, langs) = args.split();
            commands::update::run(&deps, &paths, &langs).map(|r| (r.render(), 0))
        }
        Commands::Check { args } => {
            let (paths, langs) = args.split();
            commands::check::run(&deps, &paths, &langs).map(|r| (r.render(), r.exit_code()))
        }
        Commands::Restore { location, rules } => {
            commands::restore::run(&deps, &location.paths(), &rules).map(|r| (r.render(), 0))
        }
        Commands::Uninstall { args } => {
            let (paths, langs) = args.split();
            commands::uninstall::run(&deps, &paths, &langs).map(|r| (r.render(), 0))
        }
    };
    match result {
        Ok((rendered, code)) => {
            print!("{rendered}");
            code
        }
        Err(e) => fail(&e),
    }
}
