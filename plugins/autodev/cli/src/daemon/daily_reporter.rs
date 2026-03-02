//! DailyReporter trait 정의 및 기본 구현체.
//!
//! Daemon 이벤트 루프에서 매 tick마다 `maybe_run()`을 호출한다.
//! 일간 보고서 생성, Claude 분석, GitHub 이슈 게시, Knowledge PR 생성을 수행한다.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Timelike;
use tracing::info;

use crate::components::workspace::Workspace;
use crate::domain::git_repository_factory::resolve_gh_host;
use crate::domain::repository::RepoRepository;
use crate::infrastructure::claude::Claude;
use crate::infrastructure::gh::Gh;
use crate::infrastructure::git::Git;
use crate::infrastructure::suggest_workflow::SuggestWorkflow;
use crate::queue::Database;

use super::log;

// ─── Trait ───

/// Daily Report 생성기.
///
/// `maybe_run()`: daily_report_hour에 도달했으면 보고서를 생성한다.
/// Daemon 이벤트 루프의 tick arm에서 매 tick마다 호출된다.
#[async_trait(?Send)]
pub trait DailyReporter: Send {
    /// 일간 보고서 생성 시각이면 보고서를 생성하고 게시한다.
    /// 아직 시각이 아니거나 이미 오늘 생성했으면 즉시 반환한다.
    async fn maybe_run(&mut self);
}

// ─── Default Implementation ───

/// DailyReporter의 기본 구현체.
///
/// 의존성:
/// - `gh`: GitHub 이슈 생성 + PR 생성
/// - `claude`: AI 분석 (suggestion 생성)
/// - `git` + `env`: Workspace 생성 (ensure_cloned)
/// - `sw`: cross-analysis enrichment (suggest-workflow 연동)
/// - `db`: repo_find_enabled + aggregate_daily_suggestions
pub struct DefaultDailyReporter {
    gh: Arc<dyn Gh>,
    claude: Arc<dyn Claude>,
    git: Arc<dyn Git>,
    env: Arc<dyn crate::config::Env>,
    sw: Arc<dyn SuggestWorkflow>,
    db: Database,
    log_dir: PathBuf,
    log_retention_days: u32,
    daily_report_hour: u32,
    knowledge_extraction: bool,
    last_daily_report_date: String,
}

impl DefaultDailyReporter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gh: Arc<dyn Gh>,
        claude: Arc<dyn Claude>,
        git: Arc<dyn Git>,
        env: Arc<dyn crate::config::Env>,
        sw: Arc<dyn SuggestWorkflow>,
        db: Database,
        log_dir: PathBuf,
        log_retention_days: u32,
        daily_report_hour: u32,
        knowledge_extraction: bool,
    ) -> Self {
        Self {
            gh,
            claude,
            git,
            env,
            sw,
            db,
            log_dir,
            log_retention_days,
            daily_report_hour,
            knowledge_extraction,
            last_daily_report_date: String::new(),
        }
    }
}

#[async_trait(?Send)]
impl DailyReporter for DefaultDailyReporter {
    async fn maybe_run(&mut self) {
        if !self.knowledge_extraction {
            return;
        }

        let now = chrono::Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        if now.hour() < self.daily_report_hour || self.last_daily_report_date == today {
            return;
        }

        let yesterday = (now - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let log_path = self.log_dir.join(format!("daemon.{yesterday}.log"));

        log::cleanup_old_logs(&self.log_dir, self.log_retention_days);

        if log_path.exists() {
            let stats = crate::knowledge::daily::parse_daemon_log(&log_path);
            if stats.task_count > 0 {
                let patterns = crate::knowledge::daily::detect_patterns(&stats);
                let mut report =
                    crate::knowledge::daily::build_daily_report(&yesterday, &stats, patterns);

                let ws = Workspace::new(&*self.git, &*self.env);
                if let Ok(enabled) = RepoRepository::repo_find_enabled(&self.db) {
                    if let Some(er) = enabled.first() {
                        if let Ok(base) = ws.ensure_cloned(&er.url, &er.name).await {
                            crate::knowledge::daily::enrich_with_cross_analysis(
                                &mut report,
                                &*self.sw,
                            )
                            .await;

                            let per_task = crate::knowledge::daily::aggregate_daily_suggestions(
                                &self.db, &yesterday,
                            );

                            if let Some(ks) = crate::knowledge::daily::generate_daily_suggestions(
                                &*self.claude,
                                &report,
                                &base,
                            )
                            .await
                            {
                                report.suggestions = ks.suggestions;
                            }

                            report.suggestions.extend(per_task);

                            if !report.suggestions.is_empty() {
                                let cross_patterns =
                                    crate::knowledge::daily::detect_cross_task_patterns(
                                        &report.suggestions,
                                    );
                                report.patterns.extend(cross_patterns);
                            }

                            let repo_gh_host = resolve_gh_host(&*self.env, &er.name);
                            crate::knowledge::daily::post_daily_report(
                                &*self.gh,
                                &er.name,
                                &report,
                                repo_gh_host.as_deref(),
                            )
                            .await;

                            if !report.suggestions.is_empty() {
                                crate::knowledge::daily::create_knowledge_prs(
                                    &*self.gh,
                                    &ws,
                                    &er.name,
                                    &report,
                                    repo_gh_host.as_deref(),
                                )
                                .await;
                            }
                        }
                    }
                }

                info!("daily report generated for {yesterday}");
            }
        }

        self.last_daily_report_date = today;
    }
}
