use autopilot::cmd::{
    CheckCommands, Cli, Commands, IssueCommands, ListArgs, PipelineCommands, PreflightArgs,
    StatsCommands, TaskCommands, WorktreeCommands,
};
use autopilot::store::SqliteTaskStore;
use autopilot::{cmd, fs, gh, git, github};
use clap::Parser;
use std::io::stdout;
use std::path::{Path, PathBuf};

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
        Commands::Task { command } => {
            let db_path = task_store_db_path();
            let store = match SqliteTaskStore::open(&db_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("failed to open task store at {}: {e}", db_path.display());
                    std::process::exit(2);
                }
            };
            let clock = cmd::task::default_clock();
            let svc = cmd::task::task_service(&store, &clock);
            let mut out = stdout();
            match command {
                TaskCommands::List { epic, status, json } => {
                    svc.list(&epic, status, json, &mut out)
                }
                TaskCommands::Show { task_id, json } => svc.show(&task_id, json, &mut out),
                TaskCommands::Get { task_id, json } => svc.show(&task_id, json, &mut out),
                TaskCommands::ForceStatus {
                    task_id,
                    to,
                    reason,
                } => svc.force_status(&task_id, to, reason.as_deref(), &mut out),
                TaskCommands::Add {
                    epic,
                    id,
                    title,
                    body,
                    fingerprint,
                    source,
                } => svc.add(
                    &epic,
                    &id,
                    &title,
                    body.as_deref(),
                    fingerprint.as_deref(),
                    source,
                    &mut out,
                ),
                TaskCommands::AddBatch { epic, from } => svc.add_batch(&epic, &from, &mut out),
                TaskCommands::FindByPr { pr_number, json } => {
                    svc.find_by_pr(pr_number, json, &mut out)
                }
                TaskCommands::Claim { epic, json } => svc.claim(&epic, json, &mut out),
                TaskCommands::Release { task_id } => svc.release(&task_id, &mut out),
                TaskCommands::Complete { task_id, pr } => svc.complete(&task_id, pr, &mut out),
                TaskCommands::Fail { task_id } => svc.fail(&task_id, &mut out),
                TaskCommands::Escalate { task_id, issue } => {
                    svc.escalate(&task_id, issue, &mut out)
                }
            }
        }
        Commands::Epic { command } => {
            let db_path = task_store_db_path();
            let store = match SqliteTaskStore::open(&db_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("failed to open task store at {}: {e}", db_path.display());
                    std::process::exit(2);
                }
            };
            let clock = cmd::task::default_clock();
            let svc = cmd::epic::epic_service(&store, &clock);
            let mut out = stdout();
            match command {
                cmd::epic::EpicCommands::Create(args) => {
                    svc.create(&args.name, &args.spec, args.branch.as_deref(), &mut out)
                }
                cmd::epic::EpicCommands::List(args) => svc.list(args.status, args.json, &mut out),
                cmd::epic::EpicCommands::Get(args) => svc.get(&args.name, args.json, &mut out),
                cmd::epic::EpicCommands::Status(args) => {
                    svc.status(args.name.as_deref(), args.json, &mut out)
                }
                cmd::epic::EpicCommands::Complete(args) => svc.complete(&args.name, &mut out),
                cmd::epic::EpicCommands::Abandon(args) => svc.abandon(&args.name, &mut out),
                cmd::epic::EpicCommands::Reconcile(args) => {
                    svc.reconcile(&args.name, &args.plan, &mut out)
                }
                cmd::epic::EpicCommands::FindBySpecPath(args) => {
                    svc.find_by_spec_path(&args.spec, args.json, &mut out)
                }
            }
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

fn task_store_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("AUTOPILOT_DB_PATH") {
        return PathBuf::from(p);
    }
    let dir = PathBuf::from(".autopilot");
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
    dir.join("state.db")
}
