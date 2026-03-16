use autodev::cli::cron::{seed_global_crons, seed_per_repo_crons};
use autodev::core::config::models::ClawConfig;
use autodev::core::repository::*;
use autodev::infra::db::Database;
use std::path::Path;

// ─── Helpers ───

fn open_memory_db(home: &Path) -> Database {
    let db_path = home.join("autodev.db");
    let db = Database::open(&db_path).expect("open db");
    db.initialize().expect("initialize schema");
    db
}

fn add_repo(db: &Database) -> String {
    db.repo_add("https://github.com/org/test-repo", "org/test-repo")
        .expect("add repo")
}

// ═══════════════════════════════════════════════
// seed_per_repo_crons
// ═══════════════════════════════════════════════

#[test]
fn seed_per_repo_crons_creates_three_jobs() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_memory_db(tmp.path());
    let repo_id = add_repo(&db);
    let cfg = ClawConfig::default();

    let seeded = seed_per_repo_crons(&db, tmp.path(), &repo_id, &cfg).unwrap();
    assert_eq!(seeded, 3);

    let jobs = db.cron_list(Some("org/test-repo")).unwrap();
    assert_eq!(jobs.len(), 3);

    let names: Vec<&str> = jobs.iter().map(|j| j.name.as_str()).collect();
    assert!(names.contains(&"claw-evaluate"));
    assert!(names.contains(&"gap-detection"));
    assert!(names.contains(&"knowledge-extract"));

    // All should be builtin
    for job in &jobs {
        assert!(job.builtin, "job {} should be builtin", job.name);
    }
}

#[test]
fn seed_per_repo_crons_uses_config_intervals() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_memory_db(tmp.path());
    let repo_id = add_repo(&db);
    let cfg = ClawConfig {
        schedule_interval_secs: 30,
        gap_detection_interval_secs: 1800,
        ..ClawConfig::default()
    };

    seed_per_repo_crons(&db, tmp.path(), &repo_id, &cfg).unwrap();

    let jobs = db.cron_list(Some("org/test-repo")).unwrap();
    let claw_eval = jobs.iter().find(|j| j.name == "claw-evaluate").unwrap();
    let gap_det = jobs.iter().find(|j| j.name == "gap-detection").unwrap();

    match &claw_eval.schedule {
        autodev::core::models::CronSchedule::Interval { secs } => assert_eq!(*secs, 30),
        _ => panic!("expected interval schedule"),
    }
    match &gap_det.schedule {
        autodev::core::models::CronSchedule::Interval { secs } => assert_eq!(*secs, 1800),
        _ => panic!("expected interval schedule"),
    }
}

#[test]
fn seed_per_repo_crons_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_memory_db(tmp.path());
    let repo_id = add_repo(&db);
    let cfg = ClawConfig::default();

    let seeded1 = seed_per_repo_crons(&db, tmp.path(), &repo_id, &cfg).unwrap();
    assert_eq!(seeded1, 3);

    // Second call should seed 0 (already exist)
    let seeded2 = seed_per_repo_crons(&db, tmp.path(), &repo_id, &cfg).unwrap();
    assert_eq!(seeded2, 0);

    // Still only 3 jobs
    let jobs = db.cron_list(Some("org/test-repo")).unwrap();
    assert_eq!(jobs.len(), 3);
}

#[test]
fn seed_per_repo_crons_creates_script_files() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_memory_db(tmp.path());
    let repo_id = add_repo(&db);
    let cfg = ClawConfig::default();

    seed_per_repo_crons(&db, tmp.path(), &repo_id, &cfg).unwrap();

    let crons_dir = tmp.path().join("crons");
    assert!(crons_dir.join("claw-evaluate.sh").exists());
    assert!(crons_dir.join("gap-detection.sh").exists());
    assert!(crons_dir.join("knowledge-extract.sh").exists());
}

// ═══════════════════════════════════════════════
// seed_global_crons
// ═══════════════════════════════════════════════

#[test]
fn seed_global_crons_creates_three_jobs() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_memory_db(tmp.path());

    let seeded = seed_global_crons(&db, tmp.path()).unwrap();
    assert_eq!(seeded, 3);

    let jobs = db.cron_list(None).unwrap();
    // Filter global-only (repo_id is None)
    let global_jobs: Vec<_> = jobs.iter().filter(|j| j.repo_id.is_none()).collect();
    assert_eq!(global_jobs.len(), 3);

    let names: Vec<&str> = global_jobs.iter().map(|j| j.name.as_str()).collect();
    assert!(names.contains(&"hitl-timeout"));
    assert!(names.contains(&"daily-report"));
    assert!(names.contains(&"log-cleanup"));

    for job in &global_jobs {
        assert!(job.builtin, "job {} should be builtin", job.name);
    }
}

#[test]
fn seed_global_crons_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_memory_db(tmp.path());

    let seeded1 = seed_global_crons(&db, tmp.path()).unwrap();
    assert_eq!(seeded1, 3);

    let seeded2 = seed_global_crons(&db, tmp.path()).unwrap();
    assert_eq!(seeded2, 0);

    let jobs = db.cron_list(None).unwrap();
    let global_jobs: Vec<_> = jobs.iter().filter(|j| j.repo_id.is_none()).collect();
    assert_eq!(global_jobs.len(), 3);
}

#[test]
fn seed_global_crons_creates_script_files() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_memory_db(tmp.path());

    seed_global_crons(&db, tmp.path()).unwrap();

    let crons_dir = tmp.path().join("crons");
    assert!(crons_dir.join("hitl-timeout.sh").exists());
    assert!(crons_dir.join("daily-report.sh").exists());
    assert!(crons_dir.join("log-cleanup.sh").exists());
}

// ═══════════════════════════════════════════════
// repo_remove cascades cron_jobs
// ═══════════════════════════════════════════════

#[test]
fn repo_remove_deletes_associated_cron_jobs() {
    let tmp = tempfile::tempdir().unwrap();
    let db = open_memory_db(tmp.path());
    let repo_id = add_repo(&db);
    let cfg = ClawConfig::default();

    seed_per_repo_crons(&db, tmp.path(), &repo_id, &cfg).unwrap();
    assert_eq!(db.cron_list(Some("org/test-repo")).unwrap().len(), 3);

    db.repo_remove("org/test-repo").unwrap();

    // All cron jobs for this repo should be gone
    let all_jobs = db.cron_list(None).unwrap();
    let repo_jobs: Vec<_> = all_jobs.iter().filter(|j| j.repo_id.is_some()).collect();
    assert!(repo_jobs.is_empty());
}
