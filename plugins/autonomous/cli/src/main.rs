use anyhow::Result;
use clap::{Parser, Subcommand};

mod client;
mod config;
mod consumer;
mod daemon;
mod queue;
mod scanner;
mod session;
mod tui;
mod workspace;

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
enum RepoAction {
    /// 레포 등록
    Add {
        /// 레포 URL
        url: String,
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

#[derive(Subcommand)]
enum QueueAction {
    /// 큐 상태 확인
    List {
        /// 레포 이름 (org/repo)
        repo: String,
    },
    /// 실패 항목 재시도
    Retry {
        /// 큐 아이템 ID
        id: String,
    },
    /// 큐 비우기
    Clear {
        /// 레포 이름 (org/repo)
        repo: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("autodev=info".parse()?),
        )
        .init();

    let cli = Cli::parse();
    let home = config::autodev_home();
    std::fs::create_dir_all(&home)?;

    let db_path = home.join("autodev.db");
    let db = queue::Database::open(&db_path)?;
    db.initialize()?;

    match cli.command {
        Commands::Start => daemon::start(&home).await?,
        Commands::Stop => daemon::stop(&home)?,
        Commands::Restart => {
            daemon::stop(&home).ok();
            daemon::start(&home).await?;
        }
        Commands::Status => {
            let status = client::status(&db)?;
            println!("{status}");
        }
        Commands::Dashboard => tui::run(&db).await?,
        Commands::Repo { action } => match action {
            RepoAction::Add { url } => {
                client::repo_add(&db, &url)?;
            }
            RepoAction::List => {
                let list = client::repo_list(&db)?;
                println!("{list}");
            }
            RepoAction::Config { name } => {
                client::repo_config(&name)?;
            }
            RepoAction::Remove { name } => {
                client::repo_remove(&db, &name)?;
            }
        },
        Commands::Queue { action } => match action {
            QueueAction::List { repo } => {
                let list = client::queue_list(&db, &repo)?;
                println!("{list}");
            }
            QueueAction::Retry { id } => {
                client::queue_retry(&db, &id)?;
            }
            QueueAction::Clear { repo } => {
                client::queue_clear(&db, &repo)?;
            }
        },
        Commands::Logs { repo, limit } => {
            let logs = client::logs(&db, repo.as_deref(), limit)?;
            println!("{logs}");
        }
    }

    Ok(())
}
