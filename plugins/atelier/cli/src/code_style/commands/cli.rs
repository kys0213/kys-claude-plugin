//! Errors exit 2. Output is human text parsed line by line — every rendered
//! line format is a contract.

use crate::code_style::commands;
use crate::code_style::commands::CodeStyleDeps;
use crate::code_style::core::types::{CodeStylePaths, Selection};
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
    /// Install the given languages' and apps' rules and the base rules
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
    /// Back up and remove the given languages' and apps' rules; removing the
    /// last language removes everything
    Uninstall {
        #[command(flatten)]
        args: StyleArgs,
    },
}

#[derive(Args)]
pub struct Location {
    /// Plugin root holding rules/code-style/{base,lang/<lang>,app/<app>}/
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
    /// Language directory under rules/code-style/lang (e.g. go, rust);
    /// repeatable. Install needs at least one
    #[arg(long = "lang")]
    langs: Vec<String>,
    /// App kind directory under rules/code-style/app (e.g. cli, library);
    /// repeatable. Update and check default to every installed language and
    /// app when neither --lang nor --app is given
    #[arg(long = "app")]
    apps: Vec<String>,
}

impl StyleArgs {
    fn split(self) -> (CodeStylePaths, Selection) {
        let selection = Selection {
            langs: self.langs,
            apps: self.apps,
        };
        (self.location.paths(), selection)
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
            let (paths, selection) = args.split();
            commands::install::run(&deps, &paths, &selection).map(|r| (r.render(), 0))
        }
        Commands::Update { args } => {
            let (paths, selection) = args.split();
            commands::update::run(&deps, &paths, &selection).map(|r| (r.render(), 0))
        }
        Commands::Check { args } => {
            let (paths, selection) = args.split();
            commands::check::run(&deps, &paths, &selection).map(|r| (r.render(), r.exit_code()))
        }
        Commands::Restore { location, rules } => {
            commands::restore::run(&deps, &location.paths(), &rules).map(|r| (r.render(), 0))
        }
        Commands::Uninstall { args } => {
            let (paths, selection) = args.split();
            commands::uninstall::run(&deps, &paths, &selection).map(|r| (r.render(), 0))
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
