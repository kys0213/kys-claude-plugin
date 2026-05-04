use std::sync::Arc;

use autopilot::cmd::suppress::SuppressService;
use autopilot::ports::clock::FixedClock;
use autopilot::ports::task_store::TaskStore;
use autopilot::store::InMemoryTaskStore;
use chrono::{TimeZone, Utc};

// ---------- helpers ----------

fn fixture() -> (Arc<dyn TaskStore>, FixedClock) {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    (store, clock)
}

fn capture<F>(f: F) -> (i32, String)
where
    F: FnOnce(&mut Vec<u8>) -> anyhow::Result<i32>,
{
    let mut buf: Vec<u8> = Vec::new();
    let code = f(&mut buf).expect("service call");
    (code, String::from_utf8(buf).expect("utf-8"))
}

fn expect_err<F>(f: F) -> String
where
    F: FnOnce(&mut Vec<u8>) -> anyhow::Result<i32>,
{
    let mut buf: Vec<u8> = Vec::new();
    format!("{:#}", f(&mut buf).unwrap_err())
}

// ---------- add ----------

#[test]
fn suppress_add_persists_until_window() {
    let (store, clock) = fixture();
    let svc = SuppressService::new(store.as_ref(), &clock);
    let (code, out) = capture(|w| svc.add("fp1", "unmatched_watch", "2026-01-02T00:00:00Z", w));
    assert_eq!(code, 0, "stdout: {out}");
    assert!(out.contains("fp1"), "stdout: {out}");
    assert!(out.contains("unmatched_watch"), "stdout: {out}");
    // still inside the window at fixture's clock (2026-01-01)
    let (check_code, _) = capture(|w| svc.check("fp1", "unmatched_watch", w));
    assert_eq!(check_code, 0);
}

#[test]
fn suppress_add_rejects_invalid_until_timestamp() {
    let (store, clock) = fixture();
    let svc = SuppressService::new(store.as_ref(), &clock);
    let err = expect_err(|w| svc.add("fp1", "unmatched_watch", "not-a-date", w));
    assert!(err.contains("--until"), "error: {err}");
}

// ---------- check ----------

#[test]
fn suppress_check_returns_exit_0_when_active() {
    let (store, clock) = fixture();
    let svc = SuppressService::new(store.as_ref(), &clock);
    let _ = capture(|w| svc.add("fp1", "unmatched_watch", "2026-01-02T00:00:00Z", w));
    let (code, out) = capture(|w| svc.check("fp1", "unmatched_watch", w));
    assert_eq!(code, 0, "stdout: {out}");
    assert!(out.contains("suppressed"), "stdout: {out}");
}

#[test]
fn suppress_check_returns_exit_1_after_window_expires() {
    let (store, clock) = fixture();
    let svc = SuppressService::new(store.as_ref(), &clock);
    let _ = capture(|w| svc.add("fp1", "unmatched_watch", "2026-01-02T00:00:00Z", w));
    // Advance well past --until.
    clock.advance(chrono::Duration::days(2));
    let (code, out) = capture(|w| svc.check("fp1", "unmatched_watch", w));
    assert_eq!(code, 1, "stdout: {out}");
    assert!(out.contains("not suppressed"), "stdout: {out}");
}

#[test]
fn suppress_check_returns_exit_1_when_never_suppressed() {
    let (store, clock) = fixture();
    let svc = SuppressService::new(store.as_ref(), &clock);
    let (code, out) = capture(|w| svc.check("never-set", "rejected_by_human", w));
    assert_eq!(code, 1, "stdout: {out}");
    assert!(out.contains("not suppressed"), "stdout: {out}");
}

// ---------- clear ----------

#[test]
fn suppress_clear_removes_entry() {
    let (store, clock) = fixture();
    let svc = SuppressService::new(store.as_ref(), &clock);
    let _ = capture(|w| svc.add("fp1", "unmatched_watch", "2026-01-02T00:00:00Z", w));
    let (code, out) = capture(|w| svc.clear("fp1", "unmatched_watch", w));
    assert_eq!(code, 0, "stdout: {out}");
    assert!(out.contains("cleared"), "stdout: {out}");
    // After clear, check should report not-suppressed (exit 1).
    let (check_code, _) = capture(|w| svc.check("fp1", "unmatched_watch", w));
    assert_eq!(check_code, 1);
}
