use std::collections::HashMap;
use std::path::PathBuf;

use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::core::models::{DecisionType, NewClawDecision, RepoInfo};
use crate::core::repository::{ClawDecisionRepository, CronRepository, RepoRepository};
use crate::infra::db::Database;

use super::runner::{CronExecResult, ScriptRunner};

/// Cron engine that checks for due jobs and executes their scripts.
///
/// Called from the daemon tick loop. On each tick it:
/// 1. Queries the DB for due cron jobs (active + interval elapsed)
/// 2. Checks in-memory running map to prevent duplicate execution
/// 3. Resolves repo info for per-repo jobs
/// 4. Builds env vars and executes each script
/// 5. Updates last_run_at after execution
pub struct CronEngine {
    db: Database,
    home: PathBuf,
    /// In-memory tracking of running jobs to prevent duplicate execution.
    /// Key = job ID, Value = JoinHandle of the spawned task.
    running: HashMap<String, JoinHandle<()>>,
}

impl CronEngine {
    pub fn new(db: Database, home: PathBuf) -> Self {
        Self {
            db,
            home,
            running: HashMap::new(),
        }
    }

    /// Check for due cron jobs and execute them.
    ///
    /// Jobs that are still running from a previous tick are skipped.
    /// Finished jobs are cleaned up before processing new ones.
    pub async fn tick(&mut self) -> Vec<CronExecResult> {
        // Clean up finished jobs
        self.running.retain(|_id, handle| !handle.is_finished());

        let due_jobs = match self.db.cron_find_due() {
            Ok(jobs) => jobs,
            Err(e) => {
                warn!("cron: failed to query due jobs: {e}");
                return Vec::new();
            }
        };

        if due_jobs.is_empty() {
            return Vec::new();
        }

        info!("cron: found {} due job(s)", due_jobs.len());

        // Pre-fetch enabled repos (has id field for correct matching)
        let enabled_repos = self.db.repo_find_enabled().unwrap_or_default();

        let mut results = Vec::new();

        for job in &due_jobs {
            // Skip if this job is still running from a previous tick
            if self.running.contains_key(&job.id) {
                info!(
                    "cron: skipping '{}' (still running from previous tick)",
                    job.name
                );
                continue;
            }

            let repo_info = job.repo_id.as_ref().and_then(|rid| {
                enabled_repos
                    .iter()
                    .find(|r| r.id == *rid)
                    .map(|r| RepoInfo {
                        name: r.name.clone(),
                        url: r.url.clone(),
                        enabled: true,
                    })
            });

            let env_vars = ScriptRunner::build_env_vars(&self.home, job, repo_info.as_ref());
            let result = ScriptRunner::run(&job.script_path, env_vars).await;

            // Update last_run_at regardless of success/failure
            if let Err(e) = self.db.cron_update_last_run(&job.id) {
                warn!("cron: failed to update last_run_at for '{}': {e}", job.name);
            }

            results.push(result);
        }

        results
    }

    /// Spawn a long-running job and track it in the running map.
    /// Returns true if the job was spawned, false if it was already running.
    pub fn spawn_job(&mut self, job_id: String, job_name: String, future: JoinHandle<()>) -> bool {
        if let Some(handle) = self.running.get(&job_id) {
            if !handle.is_finished() {
                info!("cron: cannot spawn '{}': already running", job_name);
                return false;
            }
        }
        self.running.insert(job_id, future);
        true
    }

    /// Mark a cron job for immediate execution on the next tick by resetting its last_run_at.
    ///
    /// This is used for event-driven triggers (e.g., task failure → claw-evaluate).
    pub fn force_trigger(&self, name: &str) {
        match self.db.cron_reset_last_run(name, None) {
            Ok(()) => info!("cron: force-triggered '{name}' for next tick"),
            Err(e) => warn!("cron: failed to force-trigger '{name}': {e}"),
        }
    }

    /// Check if a specific job is currently running.
    pub fn is_running(&self, job_id: &str) -> bool {
        self.running.get(job_id).is_some_and(|h| !h.is_finished())
    }

    /// Record a claw-evaluate cron result as a decision in the DB.
    ///
    /// Parses the script stdout to determine the decision type:
    /// - "skip: ..." → Noop (queue empty, no work)
    /// - "evaluate: ..." with exit_code 0 → Advance (evaluation completed)
    /// - "evaluate: ..." with non-zero exit → Skip (evaluation failed)
    pub fn record_claw_evaluate_decision(&self, result: &CronExecResult) {
        let repo_id = match &result.repo_id {
            Some(id) => id.clone(),
            None => {
                warn!(
                    "cron: cannot record decision for '{}': no repo_id",
                    result.job_name
                );
                return;
            }
        };

        let stdout = result.stdout.trim();

        let (decision_type, reasoning) = if stdout.starts_with("skip:") {
            let reason = stdout
                .strip_prefix("skip:")
                .unwrap_or("")
                .trim()
                .to_string();
            (DecisionType::Noop, format!("claw-evaluate: {reason}"))
        } else if result.exit_code == 0 {
            (
                DecisionType::Advance,
                "claw-evaluate completed (exit=0)".to_string(),
            )
        } else {
            (
                DecisionType::Skip,
                format!("claw-evaluate failed (exit={})", result.exit_code),
            )
        };

        let decision = NewClawDecision {
            repo_id,
            spec_id: None,
            decision_type,
            target_work_id: None,
            reasoning,
            context_json: Some(
                serde_json::json!({
                    "source": "cron",
                    "job_name": result.job_name,
                    "exit_code": result.exit_code,
                    "duration_ms": result.duration_ms,
                })
                .to_string(),
            ),
        };

        match self.db.decision_add(&decision) {
            Ok(id) => info!(
                "cron: recorded claw-evaluate decision {id} (type={})",
                decision.decision_type
            ),
            Err(e) => warn!("cron: failed to record claw-evaluate decision: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{CronSchedule, CronStatus, NewCronJob};
    use crate::core::repository::{ClawDecisionRepository, CronRepository, RepoRepository};
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, Database) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).unwrap();
        db.initialize().unwrap();
        (dir, db)
    }

    #[tokio::test]
    async fn tick_returns_empty_when_no_due_jobs() {
        let (dir, db) = setup_db();
        let mut engine = CronEngine::new(db, dir.path().to_path_buf());

        let results = engine.tick().await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn tick_executes_due_job() {
        let (dir, db) = setup_db();

        // Add an active cron job with a short interval
        db.cron_add(&NewCronJob {
            name: "echo-test".to_string(),
            repo_id: None,
            schedule: CronSchedule::Interval { secs: 0 },
            script_path: "echo cron_output".to_string(),
            builtin: false,
        })
        .unwrap();

        let mut engine = CronEngine::new(db, dir.path().to_path_buf());

        let results = engine.tick().await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].exit_code, 0);
        assert_eq!(results[0].stdout.trim(), "cron_output");
    }

    #[tokio::test]
    async fn tick_skips_paused_jobs() {
        let (dir, db) = setup_db();

        // Add job then pause it
        db.cron_add(&NewCronJob {
            name: "paused-job".to_string(),
            repo_id: None,
            schedule: CronSchedule::Interval { secs: 0 },
            script_path: "echo should_not_run".to_string(),
            builtin: false,
        })
        .unwrap();

        db.cron_set_status("paused-job", None, CronStatus::Paused)
            .unwrap();

        let mut engine = CronEngine::new(db, dir.path().to_path_buf());

        let results = engine.tick().await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn tick_updates_last_run_at_after_execution() {
        let (dir, db) = setup_db();

        db.cron_add(&NewCronJob {
            name: "update-test".to_string(),
            repo_id: None,
            schedule: CronSchedule::Interval { secs: 0 },
            script_path: "echo done".to_string(),
            builtin: false,
        })
        .unwrap();

        // Open a separate DB connection to verify the update
        let verify_db = Database::open(&dir.path().join("test.db")).unwrap();

        let mut engine = CronEngine::new(db, dir.path().to_path_buf());
        let results = engine.tick().await;
        assert_eq!(results.len(), 1);

        // Verify last_run_at was updated
        let job = verify_db.cron_show("update-test", None).unwrap().unwrap();
        assert!(job.last_run_at.is_some());
    }

    #[tokio::test]
    async fn tick_handles_script_failure_gracefully() {
        let (dir, db) = setup_db();

        db.cron_add(&NewCronJob {
            name: "fail-job".to_string(),
            repo_id: None,
            schedule: CronSchedule::Interval { secs: 0 },
            script_path: "exit 1".to_string(),
            builtin: false,
        })
        .unwrap();

        let mut engine = CronEngine::new(db, dir.path().to_path_buf());

        let results = engine.tick().await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].exit_code, 1);
    }

    #[tokio::test]
    async fn tick_does_not_rerun_job_before_interval() {
        let (dir, db) = setup_db();

        db.cron_add(&NewCronJob {
            name: "interval-job".to_string(),
            repo_id: None,
            schedule: CronSchedule::Interval { secs: 3600 }, // 1 hour
            script_path: "echo first_run".to_string(),
            builtin: false,
        })
        .unwrap();

        let mut engine = CronEngine::new(db, dir.path().to_path_buf());

        // First tick should execute
        let results = engine.tick().await;
        assert_eq!(results.len(), 1);

        // Second tick should NOT execute (interval not elapsed)
        let results = engine.tick().await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn force_trigger_resets_last_run_and_job_runs_again() {
        let (dir, db) = setup_db();

        db.cron_add(&NewCronJob {
            name: "force-test".to_string(),
            repo_id: None,
            schedule: CronSchedule::Interval { secs: 3600 }, // 1 hour
            script_path: "echo forced".to_string(),
            builtin: false,
        })
        .unwrap();

        let mut engine = CronEngine::new(db, dir.path().to_path_buf());

        // First tick runs the job
        let results = engine.tick().await;
        assert_eq!(results.len(), 1);

        // Second tick should NOT run (interval not elapsed)
        let results = engine.tick().await;
        assert!(results.is_empty());

        // Force trigger resets last_run_at
        engine.force_trigger("force-test");

        // Third tick should run again
        let results = engine.tick().await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].stdout.trim(), "forced");
    }

    #[tokio::test]
    async fn is_running_returns_false_when_no_job() {
        let (dir, db) = setup_db();
        let engine = CronEngine::new(db, dir.path().to_path_buf());

        assert!(!engine.is_running("nonexistent"));
    }

    #[tokio::test]
    async fn tick_executes_expression_schedule() {
        let (dir, db) = setup_db();

        // Use a cron expression that matches every minute ("0 * * * * * *" = every second in cron crate)
        // The `cron` crate uses 7-field expressions: sec min hour day month weekday year
        db.cron_add(&NewCronJob {
            name: "cron-expr-test".to_string(),
            repo_id: None,
            schedule: CronSchedule::Expression {
                cron: "* * * * * * *".to_string(),
            },
            script_path: "echo expression_output".to_string(),
            builtin: false,
        })
        .unwrap();

        let mut engine = CronEngine::new(db, dir.path().to_path_buf());

        // First tick should execute (never run before → due)
        let results = engine.tick().await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].exit_code, 0);
        assert_eq!(results[0].stdout.trim(), "expression_output");
    }

    #[tokio::test]
    async fn tick_skips_expression_with_invalid_cron_syntax() {
        let (dir, db) = setup_db();

        db.cron_add(&NewCronJob {
            name: "bad-expr".to_string(),
            repo_id: None,
            schedule: CronSchedule::Expression {
                cron: "not-a-cron-expression".to_string(),
            },
            script_path: "echo should_not_run".to_string(),
            builtin: false,
        })
        .unwrap();

        let mut engine = CronEngine::new(db, dir.path().to_path_buf());

        // Should skip due to invalid cron expression
        let results = engine.tick().await;
        assert!(results.is_empty());
    }

    /// Setup DB with a repo and return (dir, engine_db_path, repo_id).
    /// The engine takes ownership of one DB connection; tests open a second for verification.
    fn setup_with_repo() -> (TempDir, std::path::PathBuf, String) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).unwrap();
        db.initialize().unwrap();
        let repo_id = db
            .repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();
        (dir, db_path, repo_id)
    }

    fn make_cron_result(
        job_name: &str,
        repo_id: Option<String>,
        exit_code: i32,
        stdout: &str,
    ) -> CronExecResult {
        CronExecResult {
            job_name: job_name.to_string(),
            repo_id,
            exit_code,
            stdout: stdout.to_string(),
            stderr: String::new(),
            duration_ms: 100,
        }
    }

    #[tokio::test]
    async fn record_decision_noop_when_skip_output() {
        let (dir, db_path, repo_id) = setup_with_repo();
        let engine_db = Database::open(&db_path).unwrap();
        let engine = CronEngine::new(engine_db, dir.path().to_path_buf());

        let result = make_cron_result(
            "claw-evaluate",
            Some(repo_id),
            0,
            "skip: org/repo 큐 비어있고 HITL 없음\n",
        );
        engine.record_claw_evaluate_decision(&result);

        let verify_db = Database::open(&db_path).unwrap();
        let decisions = verify_db.decision_list(None, 10).unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].decision_type, DecisionType::Noop);
        assert!(decisions[0].reasoning.contains("claw-evaluate"));
        assert!(decisions[0].context_json.is_some());
    }

    #[tokio::test]
    async fn record_decision_advance_on_success() {
        let (dir, db_path, repo_id) = setup_with_repo();
        let engine_db = Database::open(&db_path).unwrap();
        let engine = CronEngine::new(engine_db, dir.path().to_path_buf());

        let result = make_cron_result(
            "claw-evaluate",
            Some(repo_id),
            0,
            "evaluate: org/repo (pending=2, hitl=0)\nsome agent output",
        );
        engine.record_claw_evaluate_decision(&result);

        let verify_db = Database::open(&db_path).unwrap();
        let decisions = verify_db.decision_list(None, 10).unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].decision_type, DecisionType::Advance);
        assert!(decisions[0].reasoning.contains("exit=0"));
    }

    #[tokio::test]
    async fn record_decision_skip_on_failure() {
        let (dir, db_path, repo_id) = setup_with_repo();
        let engine_db = Database::open(&db_path).unwrap();
        let engine = CronEngine::new(engine_db, dir.path().to_path_buf());

        let result = make_cron_result("claw-evaluate", Some(repo_id), 1, "error output");
        engine.record_claw_evaluate_decision(&result);

        let verify_db = Database::open(&db_path).unwrap();
        let decisions = verify_db.decision_list(None, 10).unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].decision_type, DecisionType::Skip);
        assert!(decisions[0].reasoning.contains("exit=1"));
    }

    #[tokio::test]
    async fn record_decision_skipped_without_repo_id() {
        let (dir, db_path, _repo_id) = setup_with_repo();
        let engine_db = Database::open(&db_path).unwrap();
        let engine = CronEngine::new(engine_db, dir.path().to_path_buf());

        let result = make_cron_result("claw-evaluate", None, 0, "some output");
        engine.record_claw_evaluate_decision(&result);

        // No decision should be recorded (no repo_id)
        let verify_db = Database::open(&db_path).unwrap();
        let decisions = verify_db.decision_list(None, 10).unwrap();
        assert!(decisions.is_empty());
    }

    #[tokio::test]
    async fn spawn_job_tracks_running_job() {
        let (dir, db) = setup_db();
        let mut engine = CronEngine::new(db, dir.path().to_path_buf());

        // Spawn a job that sleeps briefly
        let handle = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        });

        let spawned = engine.spawn_job("job-1".to_string(), "test-job".to_string(), handle);
        assert!(spawned);
        assert!(engine.is_running("job-1"));

        // Trying to spawn the same job should fail
        let handle2 = tokio::spawn(async {});
        let spawned2 = engine.spawn_job("job-1".to_string(), "test-job".to_string(), handle2);
        assert!(!spawned2);
    }
}
