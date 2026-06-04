//! E2E scenario tests for `autopilot check stagnation`.
//!
//! Each scenario:
//! 1. Seeds an in-memory store with one or more tasks (simhash + paths
//!    crafted to land on a specific stagnation band).
//! 2. Calls [`StagnationService::check`] for the "current" task id.
//! 3. Parses the JSON written to a buffer and asserts on `status`,
//!    `similar_tasks` count, exit code, and `recommended_persona`.
//!
//! Per `.claude/rules/rust-cli.md`, the tests stay on the public API
//! (service constructor + JSON output) so internal refactors don't break
//! them. `InMemoryTaskStore` provides scenario isolation; the same
//! conformance suite verifies sqlite parity.

use std::sync::Arc;

use atelier::autopilot::cmd::check::stagnation::{StagnationConfig, StagnationService};
use atelier::autopilot::domain::{TaskId, TaskSource};
use atelier::autopilot::ports::task_store::{NewWatchTask, TaskStore};
use atelier::autopilot::store::InMemoryTaskStore;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

const EPIC: &str = "epic-a";

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap()
}

fn seed_task(
    store: &dyn TaskStore,
    id: &str,
    title: &str,
    simhash: Option<u64>,
    paths: Option<Vec<String>>,
    fingerprint: &str,
    at: DateTime<Utc>,
) {
    let nt = NewWatchTask {
        id: TaskId::from_raw(id),
        epic_name: EPIC.to_string(),
        source: TaskSource::Human,
        fingerprint: fingerprint.to_string(),
        title: title.to_string(),
        body: None,
        simhash,
        affected_paths: paths,
    };
    store.upsert_watch_task(nt, at).expect("seed task");
}

fn run_check(store: Arc<dyn TaskStore>, task_id: &str, config: StagnationConfig) -> (i32, Value) {
    let svc = StagnationService::new(store.as_ref());
    let mut buf: Vec<u8> = Vec::new();
    let exit = svc
        .check(&TaskId::from_raw(task_id), &config, &mut buf)
        .expect("service check");
    let json: Value = serde_json::from_slice(&buf).expect("valid JSON output");
    (exit, json)
}

fn default_cfg() -> StagnationConfig {
    StagnationConfig::default()
}

// ── Scenario 1: no candidates → status=ok, exit 0 ─────────────────────

#[test]
fn no_candidates_returns_ok() {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    seed_task(
        store.as_ref(),
        "aaaaaaaaaaa1",
        "introduce middleware",
        Some(0xDEAD_BEEF_DEAD_BEEF),
        Some(vec!["src/middleware.rs".into()]),
        "fp:current",
        t0(),
    );

    let (exit, json) = run_check(store, "aaaaaaaaaaa1", default_cfg());
    assert_eq!(exit, 0, "exit code should be 0 (ok)");
    assert_eq!(json["status"], "ok");
    assert_eq!(json["similar_tasks"].as_array().unwrap().len(), 0);
    assert!(json["recommended_persona"].is_null());
}

// ── Scenario 2: simhash-only path → status=stagnation, exit 4 ─────────

#[test]
fn simhash_distance_only_triggers_stagnation() {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let base: u64 = 0xA3F2_B81C_4D5E_6F1B;
    // Three previously-attempted tasks within hamming distance ≤ 3.
    seed_task(
        store.as_ref(),
        "bbbbbbbbbbb1",
        "prior 1",
        Some(base ^ 0x01),
        None, // No paths — only simhash dimension matches.
        "fp:b1",
        t0(),
    );
    seed_task(
        store.as_ref(),
        "bbbbbbbbbbb2",
        "prior 2",
        Some(base ^ 0x03),
        None,
        "fp:b2",
        t0(),
    );
    seed_task(
        store.as_ref(),
        "bbbbbbbbbbb3",
        "prior 3",
        Some(base ^ 0x07), // 3 bits flipped → distance 3, still within T=3
        None,
        "fp:b3",
        t0(),
    );
    // Current task — same simhash → distance 0/1/2/3 to each prior.
    seed_task(
        store.as_ref(),
        "ccccccccccc1",
        "current attempt",
        Some(base),
        Some(vec!["totally/unrelated.rs".into()]),
        "fp:cur",
        t0(),
    );

    let (exit, json) = run_check(store, "ccccccccccc1", default_cfg());
    assert_eq!(exit, 4, "exit code should be 4 (stagnation)");
    assert_eq!(json["status"], "stagnation");
    assert_eq!(json["similar_tasks"].as_array().unwrap().len(), 3);
    let simhash = json["current_task"]["simhash"].as_str().unwrap();
    assert!(
        simhash.starts_with("0x"),
        "current_task.simhash hex-formatted: {simhash}"
    );
}

// ── Scenario 3: jaccard-only path → status=stagnation, exit 4 ─────────

#[test]
fn jaccard_only_triggers_stagnation() {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let shared: Vec<String> = vec![
        "src/cmd/task.rs".into(),
        "src/store/sqlite.rs".into(),
        "src/ports/task_store.rs".into(),
    ];
    // Three tasks share two of three paths → Jaccard ≥ 0.5.
    let two_of_three = vec![
        "src/cmd/task.rs".into(),
        "src/store/sqlite.rs".into(),
        "tests/extra.rs".into(),
    ];
    for (i, id) in ["ddddddddddd1", "ddddddddddd2", "ddddddddddd3"]
        .iter()
        .enumerate()
    {
        seed_task(
            store.as_ref(),
            id,
            &format!("prior {i}"),
            // Different simhash → simhash dimension does NOT match.
            Some(0xFFFF_FFFF_FFFF_FFFF ^ (i as u64)),
            Some(two_of_three.clone()),
            &format!("fp:d{i}"),
            t0(),
        );
    }
    seed_task(
        store.as_ref(),
        "eeeeeeeeeee1",
        "current attempt",
        Some(0x0000_0000_0000_0000),
        Some(shared),
        "fp:cur-e",
        t0(),
    );

    let (exit, json) = run_check(store, "eeeeeeeeeee1", default_cfg());
    assert_eq!(exit, 4, "exit code should be 4 (stagnation)");
    assert_eq!(json["status"], "stagnation");
    assert_eq!(json["similar_tasks"].as_array().unwrap().len(), 3);
    // Pattern.shared_paths surfaces the intersection (2 paths).
    let shared_paths = json["pattern"]["shared_paths"].as_array().unwrap();
    assert_eq!(
        shared_paths.len(),
        2,
        "pattern.shared_paths should list the 2 always-present files, got {shared_paths:?}"
    );
}

// ── Scenario 4: dedup — both dimensions match same task, counted once ─

#[test]
fn dedup_when_both_dimensions_match_same_task() {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let base: u64 = 0xCAFE_F00D_BABE_5678;
    let paths: Vec<String> = vec!["src/a.rs".into(), "src/b.rs".into()];

    // Two prior tasks: each matches BOTH dimensions (close simhash AND
    // overlapping paths). The set must still be 2, not 4.
    seed_task(
        store.as_ref(),
        "fffffffffff1",
        "prior 1",
        Some(base ^ 0x01),
        Some(paths.clone()),
        "fp:f1",
        t0(),
    );
    seed_task(
        store.as_ref(),
        "fffffffffff2",
        "prior 2",
        Some(base ^ 0x02),
        Some(paths.clone()),
        "fp:f2",
        t0(),
    );
    seed_task(
        store.as_ref(),
        "aaaaaaaaa999",
        "current",
        Some(base),
        Some(paths),
        "fp:cur-dedup",
        t0(),
    );

    // Lower n_threshold so 2 is enough to flag stagnation.
    let cfg = StagnationConfig {
        n_threshold: 2,
        ..default_cfg()
    };
    let (exit, json) = run_check(store, "aaaaaaaaa999", cfg);
    assert_eq!(exit, 4);
    assert_eq!(
        json["similar_tasks"].as_array().unwrap().len(),
        2,
        "task matching both simhash AND paths must be counted once, got {}",
        json["similar_tasks"]
    );
}

// ── Scenario 5: N ≥ n_escalate → status=escalate, exit 5 ──────────────

#[test]
fn meets_escalate_threshold() {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let base: u64 = 0x1111_2222_3333_4444;
    // 5 similar tasks (default n_escalate = 5).
    for i in 0..5u64 {
        seed_task(
            store.as_ref(),
            &format!("aaaaaaaaaa{i:02}"),
            &format!("prior {i}"),
            Some(base ^ (i & 0x7)),
            None,
            &format!("fp:esc-{i}"),
            t0(),
        );
    }
    seed_task(
        store.as_ref(),
        "bbbbbbbbbb99",
        "current attempt",
        Some(base),
        None,
        "fp:cur-esc",
        t0(),
    );

    let (exit, json) = run_check(store, "bbbbbbbbbb99", default_cfg());
    assert_eq!(exit, 5, "exit code should be 5 (escalate)");
    assert_eq!(json["status"], "escalate");
    let count = json["similar_tasks"].as_array().unwrap().len();
    assert!(
        count >= 5,
        "expected ≥5 similar tasks for escalate band, got {count}"
    );
}

// ── Scenario 6: persona rotation tracks consecutive_failures ──────────

#[test]
fn recommended_persona_advances_with_consecutive_failures() {
    // n=3 → consecutive=4 → idx=2 → "simplifier"
    // n=5 → consecutive=6 → idx=4 → "contrarian"
    // (n+1 mirrors `cmd::issue::filter_comments` "this attempt + priors".)
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let base: u64 = 0xDEAD_BEEF_C0FE_E000;
    for i in 0..5u64 {
        seed_task(
            store.as_ref(),
            &format!("ccccccccc{i:03}"),
            &format!("prior {i}"),
            Some(base ^ (i & 0x3)),
            None,
            &format!("fp:p{i}"),
            t0(),
        );
    }
    seed_task(
        store.as_ref(),
        "ddddddddd999",
        "current attempt",
        Some(base),
        None,
        "fp:cur-p",
        t0(),
    );

    // First, run with default thresholds to confirm `escalate` band picks
    // a late-stage persona (idx ≥ 2).
    let (_exit, json) = run_check(store.clone(), "ddddddddd999", default_cfg());
    let persona = json["recommended_persona"].as_str().unwrap();
    assert!(
        [
            "hacker",
            "researcher",
            "simplifier",
            "architect",
            "contrarian"
        ]
        .contains(&persona),
        "persona should be from the rotation table, got {persona}"
    );
    // With 5 priors, consecutive = 6, idx = 4 → "contrarian".
    assert_eq!(persona, "contrarian", "5 priors should map to PERSONAS[4]");
}
