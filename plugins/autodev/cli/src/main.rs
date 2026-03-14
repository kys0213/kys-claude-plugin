use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use autodev::core::config;
use autodev::{cli as client, daemon, infra, tui};

use infra::claude::RealClaude;
use infra::gh::RealGh;
use infra::git::RealGit;
use infra::suggest_workflow::RealSuggestWorkflow;

#[derive(Parser)]
#[command(name = "autodev", version, about = "GitHub 이슈 → PR 자동화 에이전트")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 데몬 시작
    Start {
        /// 백그라운드 데몬으로 실행
        #[arg(short = 'd', long)]
        daemon: bool,
    },
    /// 데몬 중지
    Stop,
    /// 데몬 재시작
    Restart {
        /// 백그라운드 데몬으로 실행
        #[arg(short = 'd', long)]
        daemon: bool,
    },
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
    /// 토큰 사용량 리포트
    Usage {
        /// 레포 이름 필터 (org/repo)
        #[arg(long)]
        repo: Option<String>,
        /// 시작일 (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,
        /// 이슈 번호로 필터
        #[arg(long)]
        issue: Option<i64>,
    },
    /// 스펙 관리
    Spec {
        #[command(subcommand)]
        action: SpecAction,
    },
    /// HITL (Human-in-the-Loop) 이벤트 관리
    Hitl {
        #[command(subcommand)]
        action: HitlAction,
    },
    /// 크론 잡 관리
    Cron {
        #[command(subcommand)]
        action: CronAction,
    },
    /// Claw 결정 이력 조회
    Decisions {
        #[command(subcommand)]
        action: DecisionsAction,
    },
    /// Claw 워크스페이스 관리
    Claw {
        #[command(subcommand)]
        action: ClawAction,
    },
    /// 칸반 보드 출력
    Board {
        /// 레포 이름으로 필터 (org/repo)
        #[arg(long)]
        repo: Option<String>,
        /// JSON 출력
        #[arg(long)]
        json: bool,
    },
    /// Claw 에이전트 세션 시작 (claude --cwd ~/.autodev/claw-workspace)
    Agent,
    /// Convention bootstrap — detect tech stack and generate .claude/rules/
    Convention {
        #[command(subcommand)]
        action: ConventionAction,
    },
}

#[derive(Subcommand)]
enum ConventionAction {
    /// Detect and display technology stack from a repository
    Detect {
        /// Path to the repository
        repo_path: String,
    },
    /// Generate .claude/rules/ convention files (dry-run by default)
    Bootstrap {
        /// Path to the repository
        repo_path: String,
        /// Actually write files (default: dry-run)
        #[arg(long)]
        apply: bool,
    },
}

#[derive(Subcommand)]
enum ClawAction {
    /// Claw 워크스페이스 초기화
    Init {
        /// 레포별 오버라이드 생성 (org/repo)
        #[arg(long)]
        repo: Option<String>,
    },
    /// 적용된 규칙 파일 목록
    Rules {
        /// 레포별 오버라이드 포함 (org/repo)
        #[arg(long)]
        repo: Option<String>,
    },
}

#[derive(Subcommand)]
enum QueueAction {
    /// 큐 상태 조회 (daemon.status.json 기반)
    List {
        /// 레포 이름으로 필터 (org/repo)
        #[arg(long)]
        repo: Option<String>,
        /// JSON 출력
        #[arg(long)]
        json: bool,
    },
    /// 큐 아이템을 다음 phase로 전이
    Advance {
        /// 작업 ID
        work_id: String,
    },
    /// 큐 아이템을 skip 처리
    Skip {
        /// 작업 ID
        work_id: String,
        /// skip 사유
        #[arg(long)]
        reason: Option<String>,
    },
}

#[derive(Subcommand)]
enum HitlAction {
    /// HITL 이벤트 목록 조회
    List {
        /// 레포 이름으로 필터 (org/repo)
        #[arg(long)]
        repo: Option<String>,
        /// JSON 출력
        #[arg(long)]
        json: bool,
    },
    /// HITL 이벤트 상세 조회
    Show {
        /// 이벤트 ID
        id: String,
        /// JSON 출력
        #[arg(long)]
        json: bool,
    },
    /// HITL 이벤트에 응답
    Respond {
        /// 이벤트 ID
        id: String,
        /// 선택 번호
        #[arg(long)]
        choice: Option<i32>,
        /// 메시지
        #[arg(long)]
        message: Option<String>,
    },
}

#[derive(Subcommand)]
enum CronAction {
    /// 크론 잡 목록
    List {
        /// JSON 형식으로 출력
        #[arg(long)]
        json: bool,
    },
    /// 크론 잡 추가
    Add {
        /// 잡 이름
        #[arg(long)]
        name: String,
        /// 스크립트 경로
        #[arg(long)]
        script: String,
        /// 레포 이름 (org/repo)
        #[arg(long)]
        repo: Option<String>,
        /// 실행 간격 (초)
        #[arg(long)]
        interval: Option<u64>,
        /// 크론 표현식
        #[arg(long)]
        schedule: Option<String>,
    },
    /// 크론 잡 간격 업데이트
    Update {
        /// 잡 이름
        name: String,
        /// 레포 이름 (org/repo)
        #[arg(long)]
        repo: Option<String>,
        /// 새 실행 간격 (초)
        #[arg(long)]
        interval: u64,
    },
    /// 크론 잡 일시 정지
    Pause {
        /// 잡 이름
        name: String,
        /// 레포 이름 (org/repo)
        #[arg(long)]
        repo: Option<String>,
    },
    /// 크론 잡 재개
    Resume {
        /// 잡 이름
        name: String,
        /// 레포 이름 (org/repo)
        #[arg(long)]
        repo: Option<String>,
    },
    /// 크론 잡 제거
    Remove {
        /// 잡 이름
        name: String,
        /// 레포 이름 (org/repo)
        #[arg(long)]
        repo: Option<String>,
    },
    /// 크론 잡 즉시 실행
    Trigger {
        /// 잡 이름
        name: String,
        /// 레포 이름 (org/repo)
        #[arg(long)]
        repo: Option<String>,
    },
}

#[derive(Subcommand)]
enum DecisionsAction {
    /// Claw 결정 이력 목록
    List {
        /// 레포 이름으로 필터 (org/repo)
        #[arg(long)]
        repo: Option<String>,
        /// JSON 출력
        #[arg(long)]
        json: bool,
        /// 최근 N개 항목
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },
    /// Claw 결정 상세 조회
    Show {
        /// 결정 ID
        id: String,
        /// JSON 출력
        #[arg(long)]
        json: bool,
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
    /// 레포 설정 업데이트 (기존 설정에 딥머지)
    Update {
        /// 레포 이름 (org/repo)
        name: String,
        /// 업데이트할 설정 JSON (WorkflowConfig 형식)
        #[arg(long)]
        config: String,
    },
}

#[derive(Subcommand)]
enum SpecAction {
    /// 스펙 추가
    Add {
        /// 스펙 제목
        #[arg(long)]
        title: String,
        /// 스펙 본문
        #[arg(long)]
        body: String,
        /// 레포 이름 (org/repo)
        #[arg(long)]
        repo: String,
        /// 소스 파일 경로
        #[arg(long)]
        file: Option<String>,
        /// 테스트 커맨드 (JSON 배열)
        #[arg(long)]
        test_commands: Option<String>,
        /// 수락 기준 (마크다운)
        #[arg(long)]
        acceptance_criteria: Option<String>,
    },
    /// 스펙 목록
    List {
        /// 레포 이름 필터 (org/repo)
        #[arg(long)]
        repo: Option<String>,
        /// JSON 출력
        #[arg(long)]
        json: bool,
    },
    /// 스펙 상세 조회
    Show {
        /// 스펙 ID
        id: String,
        /// JSON 출력
        #[arg(long)]
        json: bool,
    },
    /// 스펙 업데이트
    Update {
        /// 스펙 ID
        id: String,
        /// 스펙 본문
        #[arg(long)]
        body: Option<String>,
        /// 테스트 커맨드 (JSON 배열)
        #[arg(long)]
        test_commands: Option<String>,
        /// 수락 기준 (마크다운)
        #[arg(long)]
        acceptance_criteria: Option<String>,
    },
    /// 스펙 일시 중단
    Pause {
        /// 스펙 ID
        id: String,
    },
    /// 스펙 재개
    Resume {
        /// 스펙 ID
        id: String,
    },
    /// 이슈 연결
    Link {
        /// 스펙 ID
        spec_id: String,
        /// 이슈 번호
        #[arg(long)]
        issue: i64,
    },
    /// 이슈 연결 해제
    Unlink {
        /// 스펙 ID
        spec_id: String,
        /// 이슈 번호
        #[arg(long)]
        issue: i64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let env = config::RealEnv;
    let home = config::autodev_home(&env);
    std::fs::create_dir_all(&home)?;

    let is_daemon = matches!(
        cli.command,
        Commands::Start { .. } | Commands::Restart { .. }
    );
    let cfg = config::loader::load_merged(&env, None);

    // _guard must live until main() returns to flush non-blocking writer
    let _guard = if is_daemon {
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

        // 우선순위: RUST_LOG 환경변수 > YAML log_level > 기본값 "info"
        let filter = if std::env::var("RUST_LOG").is_ok() {
            tracing_subscriber::EnvFilter::from_default_env()
        } else {
            tracing_subscriber::EnvFilter::new(format!("autodev={}", cfg.daemon.log_level))
        };

        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .init();

        Some(guard)
    } else {
        // 우선순위: RUST_LOG 환경변수 > YAML log_level > 기본값 "info"
        let filter = if std::env::var("RUST_LOG").is_ok() {
            tracing_subscriber::EnvFilter::from_default_env()
        } else {
            tracing_subscriber::EnvFilter::new(format!("autodev={}", cfg.daemon.log_level))
        };

        tracing_subscriber::fmt().with_env_filter(filter).init();
        None
    };

    let db_path = home.join("autodev.db");
    let db = infra::db::Database::open(&db_path)?;
    db.initialize()?;

    // infrastructure 구현체 생성 (프로덕션)
    let gh = RealGh;
    let git = RealGit;
    let claude = RealClaude;
    let sw = RealSuggestWorkflow;

    match cli.command {
        Commands::Start {
            daemon: daemonize_flag,
        } => {
            if daemonize_flag {
                #[cfg(unix)]
                {
                    let log_dir = config::resolve_log_dir(&cfg.daemon.log_dir, &home);
                    std::fs::create_dir_all(&log_dir)?;
                    daemon::daemonize(&log_dir)?;
                }
                #[cfg(not(unix))]
                {
                    anyhow::bail!("--daemon flag is only supported on Unix systems");
                }
            }
            let env: Arc<dyn config::Env> = Arc::new(env);
            let gh: Arc<dyn infra::gh::Gh> = Arc::new(gh);
            let git: Arc<dyn infra::git::Git> = Arc::new(git);
            let claude: Arc<dyn infra::claude::Claude> = Arc::new(claude);
            let sw: Arc<dyn infra::suggest_workflow::SuggestWorkflow> = Arc::new(sw);
            daemon::start(&home, env, gh, git, claude, sw).await?;
        }
        Commands::Stop => daemon::stop(&home)?,
        Commands::Restart {
            daemon: daemonize_flag,
        } => {
            daemon::stop(&home).ok();
            if daemonize_flag {
                #[cfg(unix)]
                {
                    let log_dir = config::resolve_log_dir(&cfg.daemon.log_dir, &home);
                    std::fs::create_dir_all(&log_dir)?;
                    daemon::daemonize(&log_dir)?;
                }
                #[cfg(not(unix))]
                {
                    anyhow::bail!("--daemon flag is only supported on Unix systems");
                }
            }
            let env: Arc<dyn config::Env> = Arc::new(env);
            let gh: Arc<dyn infra::gh::Gh> = Arc::new(gh);
            let git: Arc<dyn infra::git::Git> = Arc::new(git);
            let claude: Arc<dyn infra::claude::Claude> = Arc::new(claude);
            let sw: Arc<dyn infra::suggest_workflow::SuggestWorkflow> = Arc::new(sw);
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
            RepoAction::Update { name, config } => {
                client::repo_update(&db, &env, &name, &config)?;
            }
        },
        Commands::Queue { action } => match action {
            QueueAction::List { repo, json } => {
                if json {
                    let output = client::queue::queue_list_db(&db, repo.as_deref(), true)?;
                    println!("{output}");
                } else {
                    let output = client::queue_list(&env, repo.as_deref())?;
                    println!("{output}");
                }
            }
            QueueAction::Advance { work_id } => {
                let output = client::queue::queue_advance(&db, &work_id)?;
                println!("{output}");
            }
            QueueAction::Skip { work_id, reason } => {
                let output = client::queue::queue_skip(&db, &work_id, reason.as_deref())?;
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
        Commands::Usage { repo, since, issue } => {
            let report = client::usage(&db, repo.as_deref(), since.as_deref(), issue)?;
            println!("{report}");
        }
        Commands::Spec { action } => match action {
            SpecAction::Add {
                title,
                body,
                repo,
                file,
                test_commands,
                acceptance_criteria,
            } => {
                let id = client::spec::spec_add(
                    &db,
                    &title,
                    &body,
                    &repo,
                    file.as_deref(),
                    test_commands.as_deref(),
                    acceptance_criteria.as_deref(),
                )?;
                println!("created: {id}");
            }
            SpecAction::List { repo, json } => {
                let output = client::spec::spec_list(&db, repo.as_deref(), json)?;
                println!("{output}");
            }
            SpecAction::Show { id, json } => {
                let output = client::spec::spec_show(&db, &id, json)?;
                println!("{output}");
            }
            SpecAction::Update {
                id,
                body,
                test_commands,
                acceptance_criteria,
            } => {
                client::spec::spec_update(
                    &db,
                    &id,
                    body.as_deref(),
                    test_commands.as_deref(),
                    acceptance_criteria.as_deref(),
                )?;
            }
            SpecAction::Pause { id } => {
                client::spec::spec_pause(&db, &id)?;
            }
            SpecAction::Resume { id } => {
                client::spec::spec_resume(&db, &id)?;
            }
            SpecAction::Link { spec_id, issue } => {
                client::spec::spec_link(&db, &spec_id, issue)?;
            }
            SpecAction::Unlink { spec_id, issue } => {
                client::spec::spec_unlink(&db, &spec_id, issue)?;
            }
        },
        Commands::Hitl { action } => match action {
            HitlAction::List { repo, json } => {
                let output = client::hitl::list(&db, repo.as_deref(), json)?;
                println!("{output}");
            }
            HitlAction::Show { id, json } => {
                let output = client::hitl::show(&db, &id, json)?;
                println!("{output}");
            }
            HitlAction::Respond {
                id,
                choice,
                message,
            } => {
                let output = client::hitl::respond(&db, &id, choice, message.as_deref())?;
                println!("{output}");
            }
        },
        Commands::Decisions { action } => match action {
            DecisionsAction::List { repo, json, limit } => {
                let output = client::decisions::list(&db, repo.as_deref(), limit, json)?;
                println!("{output}");
            }
            DecisionsAction::Show { id, json } => {
                let output = client::decisions::show(&db, &id, json)?;
                println!("{output}");
            }
        },
        Commands::Cron { action } => match action {
            CronAction::List { json } => {
                let output = client::cron::cron_list(&db, json)?;
                println!("{output}");
            }
            CronAction::Add {
                name,
                script,
                repo,
                interval,
                schedule,
            } => {
                client::cron::cron_add(
                    &db,
                    &name,
                    &script,
                    repo.as_deref(),
                    interval,
                    schedule.as_deref(),
                )?;
            }
            CronAction::Update {
                name,
                repo,
                interval,
            } => {
                client::cron::cron_update(&db, &name, repo.as_deref(), interval)?;
            }
            CronAction::Pause { name, repo } => {
                client::cron::cron_pause(&db, &name, repo.as_deref())?;
            }
            CronAction::Resume { name, repo } => {
                client::cron::cron_resume(&db, &name, repo.as_deref())?;
            }
            CronAction::Remove { name, repo } => {
                client::cron::cron_remove(&db, &name, repo.as_deref())?;
            }
            CronAction::Trigger { name, repo } => {
                client::cron::cron_trigger(&db, &env, &name, repo.as_deref())?;
            }
        },
        Commands::Claw { action } => match action {
            ClawAction::Init { repo } => {
                if let Some(repo_name) = repo {
                    client::claw::claw_init(&home)?;
                    client::claw::claw_init_repo(&home, &repo_name)?;
                } else {
                    client::claw::claw_init(&home)?;
                }
            }
            ClawAction::Rules { repo } => {
                let rules = client::claw::claw_rules(&home, repo.as_deref())?;
                for rule in &rules {
                    println!("  {rule}");
                }
            }
        },
        Commands::Board { repo, json } => {
            use autodev::core::board::BoardRenderer;
            use autodev::tui::board::{BoardStateBuilder, TextBoardRenderer};

            let state = BoardStateBuilder::build(&db, repo.as_deref())?;
            if json {
                let json_str = serde_json::to_string_pretty(&state)?;
                println!("{json_str}");
            } else {
                let renderer = TextBoardRenderer;
                let output = renderer.render(&state);
                print!("{output}");
            }
        }
        Commands::Agent => {
            let ws = client::claw::claw_workspace_path(&home);
            if !ws.exists() {
                anyhow::bail!("Claw workspace not initialized. Run 'autodev claw init' first.");
            }
            let status = std::process::Command::new("claude")
                .arg("--cwd")
                .arg(&ws)
                .status()
                .context("failed to launch claude. Is it installed?")?;
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Commands::Convention { action } => match action {
            ConventionAction::Detect { repo_path } => {
                let path = std::path::Path::new(&repo_path);
                if !path.is_dir() {
                    anyhow::bail!("not a directory: {repo_path}");
                }
                let stack = client::convention::detect_tech_stack(path);
                print!("{}", client::convention::format_tech_stack(&stack));
            }
            ConventionAction::Bootstrap { repo_path, apply } => {
                let path = std::path::Path::new(&repo_path);
                if !path.is_dir() {
                    anyhow::bail!("not a directory: {repo_path}");
                }
                let stack = client::convention::detect_tech_stack(path);
                print!("{}", client::convention::format_tech_stack(&stack));
                let result = client::convention::bootstrap(path, &stack, apply)?;
                print!(
                    "{}",
                    client::convention::format_bootstrap_result(&result, !apply)
                );
            }
        },
    }

    Ok(())
}
