use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config;
use crate::config::Env;
use crate::domain::repository::*;
use crate::queue::Database;

/// JSON 문자열을 serde_json::Value로 파싱
fn parse_config_json(json_str: &str) -> Result<serde_json::Value> {
    serde_json::from_str(json_str).map_err(|e| anyhow::anyhow!("invalid config JSON: {e}"))
}

/// 레포의 워크스페이스 디렉토리 경로 반환 (생성 포함)
fn ensure_workspace_dir(env: &dyn Env, name: &str) -> Result<PathBuf> {
    let ws_dir = config::workspaces_path(env).join(config::sanitize_repo_name(name));
    std::fs::create_dir_all(&ws_dir)?;
    Ok(ws_dir)
}

/// serde_json::Value를 YAML로 워크스페이스 설정 파일에 저장
fn write_workspace_config(ws_dir: &Path, value: &serde_json::Value) -> Result<PathBuf> {
    let config_path = ws_dir.join(config::CONFIG_FILENAME);
    let yaml = serde_yaml::to_string(value)?;
    std::fs::write(&config_path, yaml)?;
    Ok(config_path)
}

/// 최종 effective config 출력
fn print_effective_config(env: &dyn Env, ws_dir: Option<&Path>, name: &str) -> Result<()> {
    let effective = config::loader::load_merged(env, ws_dir);
    let yaml = serde_yaml::to_string(&effective)?;
    println!("\nEffective config for {name}:\n---\n{yaml}");
    Ok(())
}

/// 상태 요약
pub fn status(db: &Database, env: &dyn Env) -> Result<String> {
    let mut output = String::new();

    // 데몬 상태
    let home = config::autodev_home(env);
    let running = crate::daemon::pid::is_running(&home);
    output.push_str(&format!(
        "autodev daemon: {}\n\n",
        if running {
            "● running"
        } else {
            "○ stopped"
        }
    ));

    // 레포 목록
    let rows = db.repo_status_summary()?;

    output.push_str("Repositories:\n");
    if rows.is_empty() {
        output.push_str("  (no repositories registered)\n");
    } else {
        for row in &rows {
            let icon = if row.enabled { "●" } else { "○" };
            output.push_str(&format!("  {icon} {}\n", row.name));
        }
    }

    Ok(output)
}

/// URL에서 org/repo 이름 추출
fn extract_repo_name(url: &str) -> Result<String> {
    let trimmed = url.trim_end_matches('/').trim_end_matches(".git");
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() < 2 {
        anyhow::bail!("invalid repository URL: {url} (expected https://github.com/org/repo)");
    }
    let org = parts[parts.len() - 2];
    let repo = parts[parts.len() - 1];
    if org.is_empty() || repo.is_empty() {
        anyhow::bail!("invalid repository URL: {url} (org or repo name is empty)");
    }
    Ok(format!("{org}/{repo}"))
}

/// 레포 등록
pub fn repo_add(
    db: &Database,
    env: &dyn config::Env,
    url: &str,
    config_json: Option<&str>,
) -> Result<()> {
    let name = extract_repo_name(url)?;

    match db.repo_add(url, &name) {
        Ok(_) => {}
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("UNIQUE constraint failed") {
                anyhow::bail!(
                    "already registered: {name}. Use 'autodev repo config {name}' to view settings."
                );
            }
            return Err(e);
        }
    }

    if let Some(json_str) = config_json {
        let value = parse_config_json(json_str)?;
        let ws_dir = ensure_workspace_dir(env, &name)?;
        let config_path = write_workspace_config(&ws_dir, &value)?;
        println!("registered: {name} ({url})");
        println!("config: written to {}", config_path.display());
    } else {
        println!("registered: {name} ({url})");
        println!("config: edit ~/.autodev.yaml (global) or <repo>/.autodev.yaml (per-repo)");
    }

    Ok(())
}

/// 레포 목록
pub fn repo_list(db: &Database) -> Result<String> {
    let repos = db.repo_list()?;
    let mut output = String::new();

    for r in &repos {
        let icon = if r.enabled { "●" } else { "○" };
        output.push_str(&format!("{icon} {}\n  {}\n\n", r.name, r.url));
    }

    if output.is_empty() {
        output.push_str("No repositories registered. Use 'autodev repo add <url>' to add one.\n");
    }

    Ok(output)
}

/// 레포 설정 표시 (YAML 기반)
pub fn repo_config(env: &dyn Env, name: &str) -> Result<()> {
    // 글로벌 설정
    let global_path = config::loader::global_config_path(env);
    println!("Global config: {}", global_path.display());

    if global_path.exists() {
        println!("  (exists)");
    } else {
        println!("  (not found — using defaults)");
    }

    // 워크스페이스에서 레포별 설정 탐색
    let ws = config::workspaces_path(env).join(config::sanitize_repo_name(name));
    let repo_config_path = ws.join(config::CONFIG_FILENAME);
    println!("\nRepo config: {}", repo_config_path.display());

    if repo_config_path.exists() {
        println!("  (exists)");
    } else {
        println!("  (not found — using global/defaults)");
    }

    // 최종 머지 결과 표시
    print_effective_config(env, if ws.exists() { Some(&ws) } else { None }, name)?;

    Ok(())
}

/// 레포 설정 업데이트 (딥머지)
pub fn repo_update(
    db: &Database,
    env: &dyn config::Env,
    name: &str,
    config_json: &str,
) -> Result<()> {
    // 1. 레포 존재 여부 확인
    let repos = db.repo_list()?;
    if !repos.iter().any(|r| r.name == name) {
        anyhow::bail!("repository not found: {name}. Use 'autodev repo add' first.");
    }

    // 2. JSON 파싱
    let new_value = parse_config_json(config_json)?;

    // 빈 JSON 객체 체크
    if new_value.as_object().is_some_and(|m| m.is_empty()) {
        println!("warning: empty config '{{}}' — no changes applied for {name}.");
        return Ok(());
    }

    // 3. 기존 워크스페이스 YAML 로드 + 딥머지
    let ws_dir = ensure_workspace_dir(env, name)?;
    let config_path = ws_dir.join(config::CONFIG_FILENAME);

    let existing = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_yaml::from_str::<serde_json::Value>(&content).map_err(|e| {
            anyhow::anyhow!(
                "failed to parse existing config {}: {e}",
                config_path.display()
            )
        })?
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };

    let merged = config::loader::deep_merge(existing, new_value);

    // 4. YAML로 저장
    let config_path = write_workspace_config(&ws_dir, &merged)?;

    println!("updated: {name}");
    println!("config: written to {}", config_path.display());

    // 최종 effective config 표시
    print_effective_config(env, Some(&ws_dir), name)?;

    Ok(())
}

/// 레포 제거
pub fn repo_remove(db: &Database, name: &str) -> Result<()> {
    db.repo_remove(name)?;
    println!("removed: {name}");
    Ok(())
}

/// 큐 상태 조회 (daemon.status.json 기반, 조회 전용)
pub fn queue_list(env: &dyn Env, repo: Option<&str>) -> Result<String> {
    let home = config::autodev_home(env);
    let status_path = home.join("daemon.status.json");

    let status = match crate::daemon::status::read_status(&status_path) {
        Some(s) => s,
        None => {
            return Ok(
                "No queue data available (daemon not running or status file not found).\n"
                    .to_string(),
            );
        }
    };

    let mut output = String::new();
    output.push_str(&format!(
        "Queue status (updated: {})\n\n",
        status.updated_at
    ));

    let items: Vec<_> = if let Some(filter_repo) = repo {
        status
            .active_items
            .iter()
            .filter(|i| i.repo_name == filter_repo)
            .collect()
    } else {
        status.active_items.iter().collect()
    };

    if items.is_empty() {
        output.push_str("  (no active items)\n");
    } else {
        for item in &items {
            output.push_str(&format!(
                "  [{}] {}#{} — {} ({})\n",
                item.queue_type.chars().next().unwrap_or('?').to_uppercase(),
                item.repo_name,
                item.number,
                item.phase,
                item.title,
            ));
        }
    }

    output.push_str(&format!(
        "\nCounters: wip={} done={} skip={} failed={}\n",
        status.counters.wip, status.counters.done, status.counters.skip, status.counters.failed,
    ));

    Ok(output)
}

/// 글로벌 설정 표시
pub fn config_show(env: &dyn Env) -> Result<()> {
    let global_path = config::loader::global_config_path(env);
    println!("Config file: {}", global_path.display());
    if global_path.exists() {
        println!("  (exists)");
    } else {
        println!("  (not found — using defaults)");
    }

    let merged = config::loader::load_merged(env, None);
    let yaml = serde_yaml::to_string(&merged)?;
    println!("\nEffective config:\n---\n{yaml}");
    Ok(())
}

/// 사용량 리포트
pub fn usage(
    db: &Database,
    repo: Option<&str>,
    since: Option<&str>,
    issue: Option<i64>,
) -> Result<String> {
    use crate::domain::repository::TokenUsageRepository;

    if let Some(s) = since {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("invalid --since format: expected YYYY-MM-DD"))?;
    }

    let mut output = String::new();

    if let (Some(repo_name), Some(issue_num)) = (repo, issue) {
        // Issue-specific report
        let entries = db.usage_by_issue(repo_name, issue_num)?;
        if entries.is_empty() {
            output.push_str(&format!("No usage data for {repo_name}#{issue_num}.\n"));
        } else {
            output.push_str(&format!("Usage for {repo_name}#{issue_num}:\n\n"));
            for e in &entries {
                let dur = format_duration(e.duration_ms);
                output.push_str(&format!(
                    "  [{:>8}] sessions={} duration={} tokens(in={} out={})\n",
                    e.queue_type, e.sessions, dur, e.input_tokens, e.output_tokens
                ));
            }
        }
        return Ok(output);
    }

    let summary = db.usage_summary(repo, since)?;

    output.push_str("Usage Summary\n");
    output.push_str(&format!("  Total sessions:  {}\n", summary.total_sessions));
    output.push_str(&format!(
        "  Total duration:  {}\n",
        format_duration(summary.total_duration_ms)
    ));
    if summary.total_input_tokens > 0 || summary.total_output_tokens > 0 {
        output.push_str(&format!(
            "  Input tokens:    {}\n  Output tokens:   {}\n",
            summary.total_input_tokens, summary.total_output_tokens
        ));
        output.push_str(&format!(
            "  Cache write:     {}\n  Cache read:      {}\n",
            summary.total_cache_write_tokens, summary.total_cache_read_tokens
        ));
    }

    if !summary.by_queue_type.is_empty() {
        output.push_str("\nBy queue type:\n");
        for qt in &summary.by_queue_type {
            let dur = format_duration(qt.duration_ms);
            output.push_str(&format!(
                "  {:>12}: sessions={:<4} duration={:<12} tokens(in={} out={})\n",
                qt.queue_type, qt.sessions, dur, qt.input_tokens, qt.output_tokens
            ));
        }
    }

    if !summary.by_repo.is_empty() {
        output.push_str("\nBy repository:\n");
        for r in &summary.by_repo {
            let dur = format_duration(r.duration_ms);
            output.push_str(&format!(
                "  {}: sessions={} duration={} tokens(in={} out={})\n",
                r.repo_name, r.sessions, dur, r.input_tokens, r.output_tokens
            ));
        }
    }

    Ok(output)
}

fn format_duration(ms: i64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{:.1}m", ms as f64 / 60_000.0)
    } else {
        format!("{:.1}h", ms as f64 / 3_600_000.0)
    }
}

/// 로그 조회
pub fn logs(db: &Database, repo: Option<&str>, limit: usize) -> Result<String> {
    let entries = db.log_recent(repo, limit)?;
    let mut output = String::new();

    for entry in &entries {
        let status = match entry.exit_code {
            Some(0) => "✓",
            Some(_) => "✗",
            None => "…",
        };
        let dur = entry
            .duration_ms
            .map(|d| format!(" ({d}ms)"))
            .unwrap_or_default();
        output.push_str(&format!(
            "  {} [{}] {} {}{}\n",
            entry.started_at, entry.queue_type, status, entry.command, dur
        ));
    }

    if output.is_empty() {
        output.push_str("No logs found.\n");
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Env;
    use crate::domain::repository::RepoRepository;
    use std::env::VarError;

    struct TestEnv {
        home: String,
    }

    impl Env for TestEnv {
        fn var(&self, key: &str) -> Result<String, VarError> {
            match key {
                "HOME" | "AUTODEV_HOME" => Ok(self.home.clone()),
                _ => Err(VarError::NotPresent),
            }
        }
    }

    fn setup_test_db(dir: &std::path::Path) -> crate::queue::Database {
        let db_path = dir.join("test.db");
        let db = crate::queue::Database::open(&db_path).unwrap();
        db.initialize().unwrap();
        db
    }

    #[test]
    fn repo_update_existing_repo_deep_merges() {
        let tmp = tempfile::tempdir().unwrap();
        let db = setup_test_db(tmp.path());
        let env = TestEnv {
            home: tmp.path().to_string_lossy().to_string(),
        };

        // Register a repo
        db.repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        // Write initial config
        let ws_dir = config::workspaces_path(&env).join(config::sanitize_repo_name("org/repo"));
        std::fs::create_dir_all(&ws_dir).unwrap();
        std::fs::write(
            ws_dir.join(config::CONFIG_FILENAME),
            "daemon:\n  poll_interval: 30\nsources:\n  github:\n    gh_host: github.com\n",
        )
        .unwrap();

        // Update with new config
        repo_update(&db, &env, "org/repo", r#"{"daemon":{"log_level":"debug"}}"#).unwrap();

        // Read back and verify deep merge
        let content = std::fs::read_to_string(ws_dir.join(config::CONFIG_FILENAME)).unwrap();
        let value: serde_json::Value = serde_yaml::from_str(&content).unwrap();

        // Original field preserved
        assert_eq!(value["daemon"]["poll_interval"], 30);
        // New field added
        assert_eq!(value["daemon"]["log_level"], "debug");
        // Other section preserved
        assert_eq!(value["sources"]["github"]["gh_host"], "github.com");
    }

    #[test]
    fn repo_update_nonexistent_repo_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let db = setup_test_db(tmp.path());
        let env = TestEnv {
            home: tmp.path().to_string_lossy().to_string(),
        };

        let result = repo_update(&db, &env, "org/nonexistent", r#"{"key":"value"}"#);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("repository not found"),
            "expected 'repository not found', got: {err}"
        );
    }

    #[test]
    fn repo_update_invalid_json_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let db = setup_test_db(tmp.path());
        let env = TestEnv {
            home: tmp.path().to_string_lossy().to_string(),
        };

        db.repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        let result = repo_update(&db, &env, "org/repo", "not-valid-json");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("invalid config JSON"),
            "expected 'invalid config JSON', got: {err}"
        );
    }

    #[test]
    fn repo_update_empty_json_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let db = setup_test_db(tmp.path());
        let env = TestEnv {
            home: tmp.path().to_string_lossy().to_string(),
        };

        db.repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        // Write initial config
        let ws_dir = config::workspaces_path(&env).join(config::sanitize_repo_name("org/repo"));
        std::fs::create_dir_all(&ws_dir).unwrap();
        let original = "daemon:\n  poll_interval: 30\n";
        std::fs::write(ws_dir.join(config::CONFIG_FILENAME), original).unwrap();

        // Update with empty JSON
        repo_update(&db, &env, "org/repo", "{}").unwrap();

        // Config file should be unchanged (no write happened)
        let content = std::fs::read_to_string(ws_dir.join(config::CONFIG_FILENAME)).unwrap();
        assert_eq!(
            content, original,
            "config should not be modified for empty JSON"
        );
    }

    #[test]
    fn repo_update_no_existing_yaml_creates_new() {
        let tmp = tempfile::tempdir().unwrap();
        let db = setup_test_db(tmp.path());
        let env = TestEnv {
            home: tmp.path().to_string_lossy().to_string(),
        };

        db.repo_add("https://github.com/org/repo", "org/repo")
            .unwrap();

        // No existing YAML — should create new file
        repo_update(&db, &env, "org/repo", r#"{"daemon":{"log_level":"warn"}}"#).unwrap();

        let ws_dir = config::workspaces_path(&env).join(config::sanitize_repo_name("org/repo"));
        let content = std::fs::read_to_string(ws_dir.join(config::CONFIG_FILENAME)).unwrap();
        let value: serde_json::Value = serde_yaml::from_str(&content).unwrap();
        assert_eq!(value["daemon"]["log_level"], "warn");
    }
}
