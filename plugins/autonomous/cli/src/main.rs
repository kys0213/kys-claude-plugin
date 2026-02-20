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
#[command(name = "autonomous", version, about = "이벤트 기반 자율 개발 오케스트레이터")]
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
        /// JSON 설정
        #[arg(long)]
        config: Option<String>,
    },
    /// 등록된 레포 목록
    List,
    /// 레포 설정 변경
    Config {
        /// 레포 이름 (org/repo)
        name: String,
        /// JSON 설정 업데이트
        #[arg(long)]
        update: Option<String>,
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
                .add_directive("autonomous=info".parse()?),
        )
        .init();

    let cli = Cli::parse();
    let home = config::autonomous_home();
    std::fs::create_dir_all(&home)?;

    let db_path = home.join("autonomous.db");
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
            RepoAction::Add { url, config: cfg } => {
                client::repo_add(&db, &url, cfg.as_deref())?;
            }
            RepoAction::List => {
                let list = client::repo_list(&db)?;
                println!("{list}");
            }
            RepoAction::Config { name, update } => {
                client::repo_config(&db, &name, update.as_deref())?;
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
