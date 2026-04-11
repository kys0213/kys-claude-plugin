use autopilot::cmd::{
    CheckCommands, Cli, Commands, IssueCommands, PipelineCommands, PreflightArgs, WorktreeCommands,
};
use autopilot::{cmd, fs, gh, git, github};
use clap::Parser;
use std::path::Path;

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Issue { command } => match command {
            IssueCommands::DetectOverlap(args) => cmd::issue::detect_overlap(&args),
            _ => {
                let client = gh::real();
                match command {
                    IssueCommands::CheckDup { fingerprint } => {
                        cmd::issue::check_dup(client.as_ref(), &fingerprint)
                    }
                    IssueCommands::Create(args) => cmd::issue::create(client.as_ref(), &args),
                    IssueCommands::CloseResolved { label_prefix } => {
                        cmd::issue::close_resolved(client.as_ref(), &label_prefix)
                    }
                    IssueCommands::SearchSimilar(args) => {
                        cmd::issue::search_similar(client.as_ref(), &args)
                    }
                    IssueCommands::DetectOverlap(_) => unreachable!(),
                }
            }
        },
        Commands::Pipeline { command } => {
            let client = gh::real();
            match command {
                PipelineCommands::Idle { label_prefix } => {
                    cmd::pipeline::idle(client, &label_prefix)
                }
            }
        }
        Commands::Check { command } => {
            use cmd::check::spec_code::SpecCodeAnalysis;
            use cmd::check::stagnation::StagnationAnalysis;
            use cmd::check::CheckService;

            let svc = CheckService::new(
                git::real(),
                fs::real(),
                vec![
                    Box::new(SpecCodeAnalysis),
                    Box::new(StagnationAnalysis::default()),
                ],
            );
            match command {
                CheckCommands::Diff {
                    loop_name,
                    spec_paths,
                } => svc.diff(&loop_name, &spec_paths),
                CheckCommands::Mark {
                    loop_name,
                    output_hash,
                } => svc.mark(&loop_name, output_hash.as_deref()),
                CheckCommands::Status => svc.status(),
                CheckCommands::Health => svc.health(),
            }
        }
        Commands::Watch(args) => {
            let client = github::real();
            let git_client = git::real();
            let fs_client = fs::real();
            let svc = cmd::watch::WatchService::new(client, git_client, fs_client);
            svc.run(
                &args.branch,
                &args.branch_filter,
                &args.label_prefix,
                args.poll_sec,
            )
        }
        Commands::Worktree { command } => {
            let git_client = git::real();
            let svc = cmd::worktree::WorktreeService::new(git_client);
            match command {
                WorktreeCommands::Cleanup { branch } => svc.cleanup(&branch),
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
