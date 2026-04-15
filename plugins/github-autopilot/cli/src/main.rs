use autopilot::cmd::{
    CheckCommands, Cli, Commands, IssueCommands, ListArgs, PipelineCommands, PreflightArgs,
    StatsCommands, WorktreeCommands,
};
use autopilot::{cmd, fs, gh, git, github};
use clap::Parser;
use std::path::Path;

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Issue { command } => match command {
            IssueCommands::DetectOverlap(args) => cmd::issue::detect_overlap(&args),
            IssueCommands::CheckDup { fingerprint } => {
                let client = gh::real();
                cmd::issue::check_dup(client.as_ref(), &fingerprint)
            }
            IssueCommands::Create(args) => {
                let client = gh::real();
                cmd::issue::create(client.as_ref(), &args)
            }
            IssueCommands::CloseResolved { label_prefix } => {
                let client = gh::real();
                cmd::issue::close_resolved(client.as_ref(), &label_prefix)
            }
            IssueCommands::SearchSimilar(args) => {
                let client = gh::real();
                cmd::issue::search_similar(client.as_ref(), &args)
            }
            IssueCommands::FilterComments => cmd::issue::filter_comments(),
            IssueCommands::List(ListArgs {
                stage,
                label_prefix,
                require_label,
                limit,
            }) => {
                let client = gh::real();
                cmd::issue_list::list(
                    client.as_ref(),
                    &stage,
                    &label_prefix,
                    require_label.as_deref(),
                    limit,
                )
            }
            IssueCommands::ExtractFingerprint => {
                use std::io::Read as _;
                let mut input = String::new();
                std::io::stdin()
                    .read_to_string(&mut input)
                    .expect("failed to read stdin");
                let result = cmd::issue_list::extract_fingerprint(
                    &input,
                    Some(&|path: &str| std::path::Path::new(path).exists()),
                );
                let exit = if result["found"].as_bool() == Some(true) {
                    0
                } else {
                    1
                };
                println!("{result}");
                Ok(exit)
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
                    status,
                } => svc.mark(&loop_name, output_hash.as_deref(), status.as_ref()),
                CheckCommands::Status => svc.status(),
                CheckCommands::Health => svc.health(),
                CheckCommands::Reset { loop_name } => svc.reset(loop_name.as_deref()),
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
                WorktreeCommands::CleanupStale => svc.cleanup_stale_cmd(),
            }
        }
        Commands::Stats { command } => {
            let git_client = git::real();
            let fs_client = fs::real();
            let svc = cmd::stats::StatsService::new(git_client, fs_client);
            match command {
                StatsCommands::Init => svc.init(),
                StatsCommands::Update {
                    command,
                    processed,
                    success,
                    failed,
                    false_positive,
                } => svc.update(&command, processed, success, failed, false_positive),
                StatsCommands::Show { command } => svc.show(command.as_deref()),
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
