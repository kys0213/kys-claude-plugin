use std::collections::HashMap;
use std::path::PathBuf;

use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::core::models::RepoInfo;
use crate::core::repository::{CronRepository, RepoRepository};
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{CronSchedule, CronStatus, NewCronJob};
    use crate::core::repository::CronRepository;
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
