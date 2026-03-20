//! Shared helpers for E2E black-box tests.
//!
//! Each test creates its own `TempDir` as `AUTODEV_HOME`, so tests are
//! fully isolated and safe to run in parallel.

#![allow(dead_code)]

use assert_cmd::cargo_bin_cmd;
use assert_cmd::Command;
use tempfile::TempDir;

use autodev::infra::db::Database;

// ─── CLI command builder ───

/// Build a `Command` for the `autodev` binary with `AUTODEV_HOME` pointed at `home`.
pub fn autodev(home: &TempDir) -> Command {
    let mut cmd = cargo_bin_cmd!("autodev");
    cmd.env("AUTODEV_HOME", home.path());
    cmd
}

// ─── DB helpers ───

/// Open (and initialize) the SQLite database inside the given `AUTODEV_HOME`.
pub fn open_db(home: &TempDir) -> Database {
    let db_path = home.path().join("autodev.db");
    let db = Database::open(&db_path).expect("open db");
    db.initialize().expect("initialize db");
    db
}

/// Register a repo via CLI and return the repo_id from the DB.
pub fn setup_repo(home: &TempDir, url: &str) -> String {
    autodev(home).args(["repo", "add", url]).assert().success();

    // Extract repo_id from the database
    let db = open_db(home);
    let name = url
        .trim_end_matches('/')
        .rsplit('/')
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("/");

    use autodev::core::repository::RepoRepository;
    let repos = db.repo_find_enabled().expect("repo find enabled");
    repos
        .iter()
        .find(|r| r.name == name)
        .map(|r| r.id.clone())
        .expect("repo should exist after add")
}

/// Seed a queue item directly into the DB (for queue/HITL tests that need
/// pre-existing queue state not creatable via CLI alone).
pub fn seed_queue_item(
    home: &TempDir,
    repo_id: &str,
    work_id: &str,
    queue_type: &str,
    phase: &str,
    title: Option<&str>,
    github_number: i64,
) {
    let db = open_db(home);
    let now = chrono::Utc::now().to_rfc3339();
    let title_val = title.unwrap_or("Test item");
    db.conn()
        .execute(
            "INSERT INTO queue_items (work_id, repo_id, queue_type, phase, title, github_number, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            rusqlite::params![work_id, repo_id, queue_type, phase, title_val, github_number, now],
        )
        .expect("seed queue item");
}

/// Seed a PR queue item with metadata_json (review_iteration etc).
pub fn seed_pr_queue_item(
    home: &TempDir,
    repo_id: &str,
    work_id: &str,
    phase: &str,
    review_iteration: u32,
) {
    let db = open_db(home);
    let now = chrono::Utc::now().to_rfc3339();
    let metadata = serde_json::json!({
        "Pr": {
            "head_branch": "feat/test",
            "base_branch": "main",
            "review_iteration": review_iteration
        }
    });
    db.conn()
        .execute(
            "INSERT INTO queue_items (work_id, repo_id, queue_type, phase, title, github_number, metadata_json, created_at, updated_at) \
             VALUES (?1, ?2, 'pr', ?3, 'PR item', 50, ?4, ?5, ?5)",
            rusqlite::params![work_id, repo_id, phase, metadata.to_string(), now],
        )
        .expect("seed PR queue item");
}

/// Seed a HITL event directly into the DB.
pub fn seed_hitl_event(
    home: &TempDir,
    repo_id: &str,
    spec_id: Option<&str>,
    work_id: Option<&str>,
    severity: &str,
    situation: &str,
    options: &[&str],
) -> String {
    let db = open_db(home);
    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();
    let options_json = serde_json::to_string(&options).unwrap();
    db.conn()
        .execute(
            "INSERT INTO hitl_events (id, repo_id, spec_id, work_id, severity, situation, context, options, status, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, '', ?7, 'pending', ?8)",
            rusqlite::params![id, repo_id, spec_id, work_id, severity, situation, options_json, now],
        )
        .expect("seed HITL event");
    id
}

/// Seed an old HITL event for timeout testing (created_at set to `hours_ago` hours in the past).
pub fn seed_old_hitl_event(home: &TempDir, repo_id: &str, hours_ago: i64) -> String {
    let db = open_db(home);
    let old_time = (chrono::Utc::now() - chrono::Duration::hours(hours_ago)).to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();
    let options_json = serde_json::to_string(&["Option A", "Option B"]).unwrap();
    db.conn()
        .execute(
            "INSERT INTO hitl_events (id, repo_id, severity, situation, context, options, status, created_at) \
             VALUES (?1, ?2, 'medium', 'old event', '', ?3, 'pending', ?4)",
            rusqlite::params![id, repo_id, options_json, old_time],
        )
        .expect("seed old HITL event");
    id
}

// ─── JSON helpers ───

/// Run the CLI with given args and parse stdout as JSON.
pub fn run_json(home: &TempDir, args: &[&str]) -> serde_json::Value {
    let output = autodev(home).args(args).output().expect("run command");
    assert!(
        output.status.success(),
        "command failed: {:?}\nstderr: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("valid utf8");
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "failed to parse JSON from command {:?}:\n{}\nerror: {}",
            args, stdout, e
        )
    })
}

// ─── Spec helpers ───

/// Create a spec via CLI and return its ID (parsed from stdout).
pub fn create_spec(home: &TempDir, repo_name: &str, title: &str, body: &str) -> String {
    let output = autodev(home)
        .args([
            "spec", "add", "--title", title, "--body", body, "--repo", repo_name,
        ])
        .output()
        .expect("spec add");
    assert!(
        output.status.success(),
        "spec add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("valid utf8");
    // Output format: "created: <id>" or "created: ⚠ Missing sections: ...\n...\n<id>"
    // The ID is a UUID — find it in the output
    extract_uuid(&stdout).expect("should find spec UUID in output")
}

/// Create a spec with test_commands.
pub fn create_spec_with_tests(
    home: &TempDir,
    repo_name: &str,
    title: &str,
    body: &str,
    test_commands: &str,
) -> String {
    let output = autodev(home)
        .args([
            "spec",
            "add",
            "--title",
            title,
            "--body",
            body,
            "--repo",
            repo_name,
            "--test-commands",
            test_commands,
        ])
        .output()
        .expect("spec add");
    assert!(
        output.status.success(),
        "spec add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("valid utf8");
    extract_uuid(&stdout).expect("should find spec UUID in output")
}

/// Extract the first UUID (v4 pattern) from a string without regex.
pub fn extract_uuid(s: &str) -> Option<String> {
    extract_all_uuids(s).into_iter().next()
}

/// Extract all UUIDs from a string.
pub fn extract_all_uuids(s: &str) -> Vec<String> {
    let hex = |c: u8| c.is_ascii_hexdigit();
    let bytes = s.as_bytes();
    let mut results = Vec::new();
    if bytes.len() < 36 {
        return results;
    }
    let mut i = 0;
    while i + 36 <= bytes.len() {
        let candidate = &bytes[i..i + 36];
        if candidate[8] == b'-'
            && candidate[13] == b'-'
            && candidate[18] == b'-'
            && candidate[23] == b'-'
        {
            let all_hex = candidate
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != 8 && *j != 13 && *j != 18 && *j != 23)
                .all(|(_, &c)| hex(c));
            if all_hex {
                results.push(String::from_utf8_lossy(candidate).to_string());
                i += 36; // skip past this UUID
                continue;
            }
        }
        i += 1;
    }
    results
}
