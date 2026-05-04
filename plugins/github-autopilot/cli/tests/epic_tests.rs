use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use autopilot::cmd::epic::{EpicService, EpicStatusFilter};
use autopilot::domain::{Epic, EpicStatus, EventKind, TaskId, TaskStatus};
use autopilot::ports::clock::{Clock, FixedClock};
use autopilot::ports::task_store::{EpicPlan, EventFilter, NewTask, TaskStore};
use autopilot::store::InMemoryTaskStore;
use chrono::{TimeZone, Utc};
use tempfile::NamedTempFile;

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

#[test]
fn create_persists_epic_and_emits_started_event() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);

    let (code, out) = capture(|w| svc.create("e", Path::new("spec/e.md"), None, w));
    assert_eq!(code, 0, "stdout: {out}");

    let epics = store.list_epics(None).unwrap();
    assert_eq!(epics.len(), 1);
    assert_eq!(epics[0].name, "e");
    assert_eq!(epics[0].status, EpicStatus::Active);
    assert_eq!(epics[0].branch, "epic/e");
    assert_eq!(epics[0].spec_path, PathBuf::from("spec/e.md"));

    let events = store
        .list_events(EventFilter {
            kinds: vec![EventKind::EpicStarted],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].epic_name.as_deref(), Some("e"));
}

#[test]
fn create_uses_explicit_branch_override() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let (_code, _) = capture(|w| svc.create("e", Path::new("spec/e.md"), Some("custom/x"), w));
    let e = store.get_epic("e").unwrap().unwrap();
    assert_eq!(e.branch, "custom/x");
}

#[test]
fn create_returns_exit_1_when_epic_already_exists() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let _ = capture(|w| svc.create("e", Path::new("spec/e.md"), None, w));
    let (code, out) = capture(|w| svc.create("e", Path::new("spec/e.md"), None, w));
    assert_eq!(code, 1);
    assert!(out.contains("already exists"), "stdout: {out}");
}

#[test]
fn list_filters_by_status_and_renders_json() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let _ = capture(|w| svc.create("a", Path::new("spec/a.md"), None, w));
    let _ = capture(|w| svc.create("b", Path::new("spec/b.md"), None, w));
    store
        .set_epic_status("b", EpicStatus::Completed, clock.now())
        .unwrap();

    let (code, out) = capture(|w| svc.list(Some(EpicStatusFilter::Active), true, w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "a");

    let (_, out_all) = capture(|w| svc.list(Some(EpicStatusFilter::All), true, w));
    let v: serde_json::Value = serde_json::from_str(out_all.trim()).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 2);
}

#[test]
fn get_returns_exit_1_when_missing() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let (code, _) = capture(|w| svc.get("ghost", false, w));
    assert_eq!(code, 1);
}

#[test]
fn status_groups_tasks_by_state() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);

    let now = clock.now();
    store
        .insert_epic_with_tasks(
            EpicPlan {
                epic: Epic {
                    name: "e".into(),
                    spec_path: PathBuf::from("spec/e.md"),
                    branch: "epic/e".into(),
                    status: EpicStatus::Active,
                    created_at: now,
                    completed_at: None,
                },
                tasks: vec![
                    NewTask {
                        id: TaskId::from_raw("aaaaaaaaaaaa"),
                        source: autopilot::domain::TaskSource::Decompose,
                        fingerprint: None,
                        title: "first".into(),
                        body: None,
                    },
                    NewTask {
                        id: TaskId::from_raw("bbbbbbbbbbbb"),
                        source: autopilot::domain::TaskSource::Decompose,
                        fingerprint: None,
                        title: "second".into(),
                        body: None,
                    },
                ],
                deps: vec![(
                    TaskId::from_raw("bbbbbbbbbbbb"),
                    TaskId::from_raw("aaaaaaaaaaaa"),
                )],
            },
            now,
        )
        .unwrap();

    let (code, out) = capture(|w| svc.status(Some("e"), true, w));
    assert_eq!(code, 0, "stdout: {out}");
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    let r = &v.as_array().unwrap()[0];
    assert_eq!(r["epic"], "e");
    assert_eq!(r["counts"]["ready"], 1);
    assert_eq!(r["counts"]["pending"], 1);
    assert_eq!(r["total"], 2);
}

#[test]
fn complete_marks_epic_completed_and_records_event() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let _ = capture(|w| svc.create("e", Path::new("spec/e.md"), None, w));

    let (code, _) = capture(|w| svc.complete("e", w));
    assert_eq!(code, 0);
    let e = store.get_epic("e").unwrap().unwrap();
    assert_eq!(e.status, EpicStatus::Completed);
    let events = store
        .list_events(EventFilter {
            kinds: vec![EventKind::EpicCompleted],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(events.len(), 1);
}

#[test]
fn complete_returns_exit_1_when_epic_missing() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let (code, out) = capture(|w| svc.complete("ghost", w));
    assert_eq!(code, 1);
    assert!(out.contains("not found"));
}

#[test]
fn abandon_marks_epic_abandoned() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let _ = capture(|w| svc.create("e", Path::new("spec/e.md"), None, w));
    let (code, _) = capture(|w| svc.abandon("e", w));
    assert_eq!(code, 0);
    assert_eq!(
        store.get_epic("e").unwrap().unwrap().status,
        EpicStatus::Abandoned
    );
}

#[test]
fn find_by_spec_path_matches_active_epic() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let _ = capture(|w| svc.create("e", Path::new("spec/e.md"), None, w));
    let (code, out) = capture(|w| svc.find_by_spec_path(Path::new("spec/e.md"), true, w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["name"], "e");
}

#[test]
fn find_by_spec_path_returns_exit_1_when_no_match() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let (code, _) = capture(|w| svc.find_by_spec_path(Path::new("spec/none.md"), false, w));
    assert_eq!(code, 1);
}

#[test]
fn find_by_spec_path_returns_exit_3_on_inconsistency() {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let clock = FixedClock::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    let now = clock.now();
    for name in ["a", "b"] {
        store
            .insert_epic_with_tasks(
                EpicPlan {
                    epic: Epic {
                        name: name.into(),
                        spec_path: PathBuf::from("spec/shared.md"),
                        branch: format!("epic/{name}"),
                        status: EpicStatus::Active,
                        created_at: now,
                        completed_at: None,
                    },
                    tasks: vec![],
                    deps: vec![],
                },
                now,
            )
            .unwrap();
    }
    let svc = EpicService::new(store.as_ref(), &clock);
    let (code, out) = capture(|w| svc.find_by_spec_path(Path::new("spec/shared.md"), false, w));
    assert_eq!(code, 3);
    assert!(out.contains("inconsistency"), "stdout: {out}");
}

#[test]
fn reconcile_applies_jsonl_plan_and_is_idempotent() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let _ = capture(|w| svc.create("e", Path::new("spec/e.md"), None, w));

    let plan = NamedTempFile::new().unwrap();
    let lines = [
        r#"{"kind":"task","id":"aaaaaaaaaaaa","title":"first","fingerprint":null,"source":"decompose"}"#,
        r#"{"kind":"task","id":"bbbbbbbbbbbb","title":"second"}"#,
        r#"{"kind":"dep","task":"bbbbbbbbbbbb","depends_on":"aaaaaaaaaaaa"}"#,
    ];
    {
        let mut f = std::fs::File::create(plan.path()).unwrap();
        for l in &lines {
            writeln!(f, "{l}").unwrap();
        }
    }

    let (code1, _) = capture(|w| svc.reconcile("e", plan.path(), w));
    assert_eq!(code1, 0);
    let tasks = store.list_tasks_by_epic("e", None).unwrap();
    assert_eq!(tasks.len(), 2);
    let by_id = |id: &str| tasks.iter().find(|t| t.id.as_str() == id).cloned().unwrap();
    assert_eq!(by_id("aaaaaaaaaaaa").status, TaskStatus::Ready);
    assert_eq!(by_id("bbbbbbbbbbbb").status, TaskStatus::Pending);

    let snapshot1 = store.list_tasks_by_epic("e", None).unwrap();

    let (code2, _) = capture(|w| svc.reconcile("e", plan.path(), w));
    assert_eq!(code2, 0);
    let snapshot2 = store.list_tasks_by_epic("e", None).unwrap();
    assert_eq!(
        snapshot1
            .iter()
            .map(|t| (&t.id, t.status))
            .collect::<Vec<_>>(),
        snapshot2
            .iter()
            .map(|t| (&t.id, t.status))
            .collect::<Vec<_>>()
    );

    let reconciled_events = store
        .list_events(EventFilter {
            kinds: vec![EventKind::Reconciled],
            ..Default::default()
        })
        .unwrap();
    assert_eq!(reconciled_events.len(), 2);
}

#[test]
fn reconcile_returns_exit_1_when_epic_missing() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let plan = NamedTempFile::new().unwrap();
    let (code, out) = capture(|w| svc.reconcile("ghost", plan.path(), w));
    assert_eq!(code, 1);
    assert!(out.contains("not found"));
}
