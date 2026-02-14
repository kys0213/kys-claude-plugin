mod helpers;

use helpers::{cli_with_home, setup_project};
use predicates::prelude::*;

// --- E1: --project creates DB at encoded path ---
#[test]
fn e1_project_flag_creates_db_at_encoded_path() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);

    cli_with_home(&tmp)
        .args(["index", "--project", project.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("DB:"));

    // Verify DB was created in ~/.claude/suggest-workflow-index/{encoded}/index.db
    let canonical = project.canonicalize().unwrap();
    let normalized = canonical
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    let encoded = format!("-{}", &normalized[1..].replace('/', "-"));

    let db_path = tmp
        .path()
        .join(".claude")
        .join("suggest-workflow-index")
        .join(&encoded)
        .join("index.db");

    assert!(db_path.exists(), "DB should exist at {}", db_path.display());
}

// --- E2: --db creates DB at specified path ---
#[test]
fn e2_db_flag_creates_db_at_specified_path() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);

    let custom_db = tmp.path().join("custom_location").join("my.db");

    cli_with_home(&tmp)
        .args([
            "index",
            "--project",
            project.to_str().unwrap(),
            "--db",
            custom_db.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(
        custom_db.exists(),
        "DB should exist at {}",
        custom_db.display()
    );
}

// --- E3: --project omitted uses cwd ---
#[test]
fn e3_project_omitted_uses_cwd() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);

    // Set current_dir to the project path so it's used as default
    cli_with_home(&tmp)
        .current_dir(&project)
        .args(["index"])
        .assert()
        .success()
        .stderr(predicate::str::contains("DB:"));
}

// --- E4: --db overrides --project for DB path ---
#[test]
fn e4_db_flag_overrides_project_for_db_path() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);

    let custom_db = tmp.path().join("override.db");

    cli_with_home(&tmp)
        .args([
            "index",
            "--project",
            project.to_str().unwrap(),
            "--db",
            custom_db.to_str().unwrap(),
        ])
        .assert()
        .success();

    // --db path should be used
    assert!(
        custom_db.exists(),
        "DB should exist at overridden path {}",
        custom_db.display()
    );

    // Default encoded path should NOT have a DB
    let canonical = project.canonicalize().unwrap();
    let normalized = canonical
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string();
    let encoded = format!("-{}", &normalized[1..].replace('/', "-"));
    let default_db = tmp
        .path()
        .join(".claude")
        .join("suggest-workflow-index")
        .join(&encoded)
        .join("index.db");

    assert!(
        !default_db.exists(),
        "Default DB should NOT exist when --db is specified"
    );
}

// --- E5: Schema version mismatch detection ---
#[test]
fn e5_schema_version_mismatch_handled() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);

    let custom_db = tmp.path().join("version_test.db");

    // First: create a valid DB
    cli_with_home(&tmp)
        .args([
            "index",
            "--project",
            project.to_str().unwrap(),
            "--db",
            custom_db.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Tamper with schema_version to simulate a newer DB
    let conn = rusqlite::Connection::open(&custom_db).unwrap();
    conn.execute(
        "UPDATE meta SET value = '999' WHERE key = 'schema_version'",
        [],
    )
    .unwrap();
    drop(conn);

    // Re-run index — should fail with version mismatch
    cli_with_home(&tmp)
        .args([
            "index",
            "--project",
            project.to_str().unwrap(),
            "--db",
            custom_db.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("newer than this CLI"));
}

// --- E6: --full recovers from version mismatch ---
#[test]
fn e6_full_flag_recovers_from_version_mismatch() {
    let (tmp, project) = setup_project(&["minimal.jsonl"]);

    let custom_db = tmp.path().join("recover_test.db");

    // Create a valid DB
    cli_with_home(&tmp)
        .args([
            "index",
            "--project",
            project.to_str().unwrap(),
            "--db",
            custom_db.to_str().unwrap(),
        ])
        .assert()
        .success();

    // Tamper with schema_version
    let conn = rusqlite::Connection::open(&custom_db).unwrap();
    conn.execute(
        "UPDATE meta SET value = '999' WHERE key = 'schema_version'",
        [],
    )
    .unwrap();
    drop(conn);

    // --full should delete and rebuild
    cli_with_home(&tmp)
        .args([
            "index",
            "--full",
            "--project",
            project.to_str().unwrap(),
            "--db",
            custom_db.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("1 new"));
}
