use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod components;
mod config;
mod daemon;
mod infrastructure;
mod knowledge;
mod pipeline;
mod queue;
mod scanner;
mod tui;

use infrastructure::claude::RealClaude;
use infrastructure::gh::RealGh;
use infrastructure::git::RealGit;
use infrastructure::suggest_workflow::RealSuggestWorkflow;

#[derive(Parser)]
#[command(name = "autodev", version, about = "GitHub 이슈 → PR 자동화 에이전트")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 데몬 시작 (백그라운드)
    Start,
    /// 데몬 중지
    Stop,
    /// 데몬 재시작
    Restart,
    /// 상태 요약 출력
    Status,
    /// TUI 대시보드
    Dashboard,
    /// 레포 관리
    Repo {
        #[command(subcommand)]
        action: RepoAction,
    },
    /// 큐 관리
    Queue {
        #[command(subcommand)]
        action: QueueAction,
    },
    /// 설정 관리
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// 실행 로그 조회
    Logs {
        /// 레포 이름 (org/repo)
        repo: Option<String>,
        /// 최근 N개 항목
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },
}

#[derive(Subcommand)]
enum QueueAction {
    /// 큐 상태 조회 (daemon.status.json 기반)
    List {
        /// 레포 이름으로 필터 (org/repo)
        repo: Option<String>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// 현재 설정 표시 (글로벌 + 기본값 머지 결과)
    Show,
}

#[derive(Subcommand)]
enum RepoAction {
    /// 레포 등록
    Add {
        /// 레포 URL
        url: String,
        /// 초기 설정 JSON (WorkflowConfig 형식)
        #[arg(long)]
        config: Option<String>,
    },
    /// 등록된 레포 목록
    List,
    /// 레포 설정 확인 (YAML 기반)
    Config {
        /// 레포 이름 (org/repo)
        name: String,
    },
    /// 레포 제거
    Remove {
        /// 레포 이름 (org/repo)
        name: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let env = config::RealEnv;
    let home = config::autodev_home(&env);
    std::fs::create_dir_all(&home)?;

    let is_daemon = matches!(cli.command, Commands::Start | Commands::Restart);

    // _guard must live until main() returns to flush non-blocking writer
    let _guard = if is_daemon {
        let cfg = config::loader::load_merged(&env, None);
        let log_dir = config::resolve_log_dir(&cfg.daemon.log_dir, &home);
        std::fs::create_dir_all(&log_dir)?;

        let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("daemon")
            .filename_suffix("log")
            .build(&log_dir)
            .expect("failed to create log appender");

        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("autodev=info".parse()?),
            )
            .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .init();

        Some(guard)
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::from_default_env()
                    .add_directive("autodev=info".parse()?),
            )
            .init();
        None
    };

    let db_path = home.join("autodev.db");
    let db = queue::Database::open(&db_path)?;
    db.initialize()?;

    // infrastructure 구현체 생성 (프로덕션)
    let gh = RealGh;
    let git = RealGit;
    let claude = RealClaude;
    let sw = RealSuggestWorkflow;

    match cli.command {
        Commands::Start => {
            let env: Arc<dyn config::Env> = Arc::new(env);
            let gh: Arc<dyn infrastructure::gh::Gh> = Arc::new(gh);
            let git: Arc<dyn infrastructure::git::Git> = Arc::new(git);
            let claude: Arc<dyn infrastructure::claude::Claude> = Arc::new(claude);
            let sw: Arc<dyn infrastructure::suggest_workflow::SuggestWorkflow> = Arc::new(sw);
            daemon::start(&home, env, gh, git, claude, sw).await?;
        }
        Commands::Stop => daemon::stop(&home)?,
        Commands::Restart => {
            daemon::stop(&home).ok();
            let env: Arc<dyn config::Env> = Arc::new(env);
            let gh: Arc<dyn infrastructure::gh::Gh> = Arc::new(gh);
            let git: Arc<dyn infrastructure::git::Git> = Arc::new(git);
            let claude: Arc<dyn infrastructure::claude::Claude> = Arc::new(claude);
            let sw: Arc<dyn infrastructure::suggest_workflow::SuggestWorkflow> = Arc::new(sw);
            daemon::start(&home, env, gh, git, claude, sw).await?;
        }
        Commands::Status => {
            let status = client::status(&db, &env)?;
            println!("{status}");
        }
        Commands::Dashboard => tui::run(&db).await?,
        Commands::Repo { action } => match action {
            RepoAction::Add { url, config } => {
                client::repo_add(&db, &env, &url, config.as_deref())?;
            }
            RepoAction::List => {
                let list = client::repo_list(&db)?;
                println!("{list}");
            }
            RepoAction::Config { name } => {
                client::repo_config(&env, &name)?;
            }
            RepoAction::Remove { name } => {
                client::repo_remove(&db, &name)?;
            }
        },
        Commands::Queue { action } => match action {
            QueueAction::List { repo } => {
                let output = client::queue_list(&env, repo.as_deref())?;
                println!("{output}");
            }
        },
        Commands::Config { action } => match action {
            ConfigAction::Show => {
                client::config_show(&env)?;
            }
        },
        Commands::Logs { repo, limit } => {
            let logs = client::logs(&db, repo.as_deref(), limit)?;
            println!("{logs}");
        }
    }

    Ok(())
}
