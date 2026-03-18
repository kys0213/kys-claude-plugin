use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use crate::core::models::{CronJob, RepoInfo};

/// Result of executing a cron script.
#[derive(Debug)]
pub struct CronExecResult {
    pub job_name: String,
    pub repo_id: Option<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Executes cron scripts with environment variable injection.
pub struct ScriptRunner;

impl ScriptRunner {
    /// Execute a script with AUTODEV_* environment variables injected.
    pub async fn run(script_path: &str, env_vars: HashMap<String, String>) -> CronExecResult {
        let start = Instant::now();
        let job_name = Path::new(script_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(script_path)
            .to_string();

        let result = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(script_path)
            .envs(&env_vars)
            .output()
            .await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => CronExecResult {
                job_name,
                repo_id: env_vars.get("AUTODEV_REPO_ID").cloned(),
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                duration_ms,
            },
            Err(e) => CronExecResult {
                job_name,
                repo_id: env_vars.get("AUTODEV_REPO_ID").cloned(),
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("failed to execute script: {e}"),
                duration_ms,
            },
        }
    }

    /// Build environment variables for a cron job.
    ///
    /// Global variables (always set):
    /// - AUTODEV_HOME
    /// - AUTODEV_DB
    /// - AUTODEV_JOB_NAME
    /// - AUTODEV_JOB_ID
    ///
    /// Per-repo variables (when repo_info is provided):
    /// - AUTODEV_REPO_ID
    /// - AUTODEV_REPO_NAME
    /// - AUTODEV_REPO_URL
    pub fn build_env_vars(
        home: &Path,
        job: &CronJob,
        repo_info: Option<&RepoInfo>,
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();

        // Global variables
        vars.insert(
            "AUTODEV_HOME".to_string(),
            home.to_string_lossy().to_string(),
        );
        vars.insert(
            "AUTODEV_DB".to_string(),
            home.join("autodev.db").to_string_lossy().to_string(),
        );
        vars.insert("AUTODEV_JOB_NAME".to_string(), job.name.clone());
        vars.insert("AUTODEV_JOB_ID".to_string(), job.id.clone());

        // Global workspace paths
        let claw_workspace = home.join("claw-workspace");
        vars.insert(
            "AUTODEV_CLAW_WORKSPACE".to_string(),
            claw_workspace.to_string_lossy().to_string(),
        );

        // Per-repo variables
        if let Some(repo) = repo_info {
            vars.insert("AUTODEV_REPO_NAME".to_string(), repo.name.clone());
            vars.insert("AUTODEV_REPO_URL".to_string(), repo.url.clone());

            let sanitized = crate::core::config::sanitize_repo_name(&repo.name);
            let workspace = home.join("workspaces").join(&sanitized);
            vars.insert(
                "AUTODEV_WORKSPACE".to_string(),
                workspace.to_string_lossy().to_string(),
            );
            vars.insert(
                "AUTODEV_REPO_ROOT".to_string(),
                workspace.join("main").to_string_lossy().to_string(),
            );
        }

        if let Some(ref repo_id) = job.repo_id {
            vars.insert("AUTODEV_REPO_ID".to_string(), repo_id.clone());
        }

        vars
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{CronJob, CronSchedule, CronStatus, RepoInfo};
    use std::path::PathBuf;

    fn sample_job() -> CronJob {
        CronJob {
            id: "job-123".to_string(),
            name: "test-job".to_string(),
            repo_id: None,
            schedule: CronSchedule::Interval { secs: 300 },
            script_path: "/tmp/test.sh".to_string(),
            status: CronStatus::Active,
            builtin: false,
            last_run_at: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn sample_repo_info() -> RepoInfo {
        RepoInfo {
            name: "my-repo".to_string(),
            url: "https://github.com/org/my-repo".to_string(),
            enabled: true,
        }
    }

    // ═══════════════════════════════════════════════
    // build_env_vars tests
    // ═══════════════════════════════════════════════

    #[test]
    fn build_env_vars_includes_global_vars() {
        let home = PathBuf::from("/home/autodev");
        let job = sample_job();
        let vars = ScriptRunner::build_env_vars(&home, &job, None);

        assert_eq!(vars.get("AUTODEV_HOME").unwrap(), "/home/autodev");
        assert_eq!(vars.get("AUTODEV_DB").unwrap(), "/home/autodev/autodev.db");
        assert_eq!(vars.get("AUTODEV_JOB_NAME").unwrap(), "test-job");
        assert_eq!(vars.get("AUTODEV_JOB_ID").unwrap(), "job-123");
    }

    #[test]
    fn build_env_vars_excludes_repo_vars_when_no_repo() {
        let home = PathBuf::from("/home/autodev");
        let job = sample_job();
        let vars = ScriptRunner::build_env_vars(&home, &job, None);

        assert!(!vars.contains_key("AUTODEV_REPO_NAME"));
        assert!(!vars.contains_key("AUTODEV_REPO_URL"));
        assert!(!vars.contains_key("AUTODEV_REPO_ID"));
    }

    #[test]
    fn build_env_vars_includes_repo_vars_when_provided() {
        let home = PathBuf::from("/home/autodev");
        let mut job = sample_job();
        job.repo_id = Some("repo-456".to_string());
        let repo = sample_repo_info();
        let vars = ScriptRunner::build_env_vars(&home, &job, Some(&repo));

        assert_eq!(vars.get("AUTODEV_REPO_NAME").unwrap(), "my-repo");
        assert_eq!(
            vars.get("AUTODEV_REPO_URL").unwrap(),
            "https://github.com/org/my-repo"
        );
        assert_eq!(vars.get("AUTODEV_REPO_ID").unwrap(), "repo-456");
    }

    // ═══════════════════════════════════════════════
    // run tests
    // ═══════════════════════════════════════════════

    #[tokio::test]
    async fn run_captures_stdout_from_script() {
        let mut env_vars = HashMap::new();
        env_vars.insert("AUTODEV_HOME".to_string(), "/tmp".to_string());

        let result = ScriptRunner::run("echo hello", env_vars).await;

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "hello");
        assert!(result.stderr.is_empty());
    }

    #[tokio::test]
    async fn run_returns_nonzero_exit_code_on_failure() {
        let env_vars = HashMap::new();

        let result = ScriptRunner::run("exit 42", env_vars).await;

        assert_eq!(result.exit_code, 42);
    }

    #[tokio::test]
    async fn run_handles_missing_script_gracefully() {
        let env_vars = HashMap::new();

        let result = ScriptRunner::run("/nonexistent/path/to/script.sh", env_vars).await;

        // Script should fail (either exit code 127 for not found, or -1 for exec error)
        assert_ne!(result.exit_code, 0);
    }

    #[tokio::test]
    async fn run_captures_stderr() {
        let env_vars = HashMap::new();

        let result = ScriptRunner::run("echo error_msg >&2", env_vars).await;

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stderr.trim(), "error_msg");
    }

    #[tokio::test]
    async fn run_injects_env_vars_into_script() {
        let mut env_vars = HashMap::new();
        env_vars.insert("AUTODEV_HOME".to_string(), "/my/home".to_string());

        let result = ScriptRunner::run("echo $AUTODEV_HOME", env_vars).await;

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout.trim(), "/my/home");
    }
}
