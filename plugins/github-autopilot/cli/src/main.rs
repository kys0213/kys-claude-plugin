use autopilot::cmd::{
    CheckCommands, Cli, Commands, IssueCommands, PipelineCommands, PreflightArgs,
};
use autopilot::{cmd, fs, gh, git};
use clap::Parser;
use std::path::Path;

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Issue { command } => {
            let client = gh::real();
            match command {
                IssueCommands::CheckDup { fingerprint } => {
                    cmd::issue::check_dup(client.as_ref(), &fingerprint)
                }
                IssueCommands::Create(args) => cmd::issue::create(client.as_ref(), &args),
                IssueCommands::CloseResolved { label_prefix } => {
                    cmd::issue::close_resolved(client.as_ref(), &label_prefix)
                }
            }
        }
        Commands::Pipeline { command } => {
            let client = gh::real();
            match command {
                PipelineCommands::Idle { label_prefix } => {
                    cmd::pipeline::idle(client, &label_prefix)
                }
            }
        }
        Commands::Check { command } => {
            let git_client = git::real();
            let fs_client = fs::real();
            match command {
                CheckCommands::Diff {
                    loop_name,
                    spec_paths,
                } => cmd::check::diff(
                    git_client.as_ref(),
                    fs_client.as_ref(),
                    &loop_name,
                    &spec_paths,
                ),
                CheckCommands::Mark { loop_name } => {
                    cmd::check::mark(git_client.as_ref(), fs_client.as_ref(), &loop_name)
                }
                CheckCommands::Status => {
                    cmd::check::status(git_client.as_ref(), fs_client.as_ref())
                }
            }
        }
        Commands::Preflight(PreflightArgs { config, repo_root }) => {
            let client = gh::real();
            let git_client = git::real();
            let fs_client = fs::real();
            cmd::preflight::run(
                client.as_ref(),
                git_client.as_ref(),
                fs_client.as_ref(),
                &config,
                Path::new(&repo_root),
            )
        }
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(2);
        }
    }
}
