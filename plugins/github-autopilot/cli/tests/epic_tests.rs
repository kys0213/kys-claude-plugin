use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use autopilot::cmd::epic::{EpicService, EpicStatusFilter};
use autopilot::domain::{Epic, EpicStatus, EventKind, TaskId, TaskStatus};
use autopilot::ports::clock::{Clock, FixedClock};
use autopilot::ports::task_store::{EpicPlan, EventFilter, NewTask, TaskStore};
use autopilot::store::InMemoryTaskStore;
use chrono::{TimeZone, Utc};
use tempfile::{NamedTempFile, TempDir};

/// Per-test scratch dir that materializes spec files on disk so
/// `EpicService::create` (which now validates `--spec` existence) accepts
/// them. Holds the `TempDir` so the files persist for the test's lifetime.
struct SpecDir {
    dir: TempDir,
}

impl SpecDir {
    fn new() -> Self {
        Self {
            dir: TempDir::new().expect("create tempdir for spec files"),
        }
    }

    /// Create an empty spec file at `<tempdir>/spec/<name>.md` and return
    /// the absolute path. The ledger never reads the file's contents — the
    /// validation gate only checks existence.
    fn make(&self, name: &str) -> PathBuf {
        let abs = self.dir.path().join("spec").join(format!("{name}.md"));
        if let Some(parent) = abs.parent() {
            std::fs::create_dir_all(parent).expect("create spec parent dir");
        }
        std::fs::File::create(&abs).expect("create spec file");
        abs
    }
}

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

fn seed_epic(svc: &EpicService, specs: &SpecDir, name: &str) {
    let path = specs.make(name);
    let _ = capture(|w| svc.create(name, &path, None, w));
}

fn write_plan_jsonl(lines: &[&str]) -> NamedTempFile {
    let f = NamedTempFile::new().unwrap();
    let mut h = std::fs::File::create(f.path()).unwrap();
    for l in lines {
        writeln!(h, "{l}").unwrap();
    }
    f
}

#[test]
fn create_persists_epic_and_emits_started_event() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    let spec = specs.make("e");

    let (code, out) = capture(|w| svc.create("e", &spec, None, w));
    assert_eq!(code, 0, "stdout: {out}");

    let epics = store.list_epics(None).unwrap();
    assert_eq!(epics.len(), 1);
    assert_eq!(epics[0].name, "e");
    assert_eq!(epics[0].status, EpicStatus::Active);
    assert_eq!(epics[0].branch, "epic/e");
    assert_eq!(epics[0].spec_path, spec);

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
    let specs = SpecDir::new();
    let spec = specs.make("e");
    let (_code, _) = capture(|w| svc.create("e", &spec, Some("custom/x"), w));
    let e = store.get_epic("e").unwrap().unwrap();
    assert_eq!(e.branch, "custom/x");
}

#[test]
fn create_returns_exit_1_when_epic_already_exists() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    let spec = specs.make("e");
    let _ = capture(|w| svc.create("e", &spec, None, w));
    let (code, out) = capture(|w| svc.create("e", &spec, None, w));
    assert_eq!(code, 1);
    assert!(out.contains("already exists"), "stdout: {out}");
}

#[test]
fn create_idempotent_creates_when_epic_missing() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    let spec = specs.make("e");

    let (code, out) = capture(|w| svc.create_with_options("e", &spec, None, true, w));
    assert_eq!(code, 0, "stdout: {out}");
    assert!(out.contains("created"), "stdout: {out}");
    let e = store.get_epic("e").unwrap().unwrap();
    assert_eq!(e.spec_path, spec);
}

#[test]
fn create_idempotent_succeeds_when_epic_exists_with_same_spec() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    let spec = specs.make("e");
    let _ = capture(|w| svc.create("e", &spec, None, w));

    let (code, out) = capture(|w| svc.create_with_options("e", &spec, None, true, w));
    assert_eq!(code, 0, "stdout: {out}");
    assert!(out.contains("already exists (idempotent)"), "stdout: {out}");
    // Single epic — no duplicate row inserted.
    assert_eq!(store.list_epics(None).unwrap().len(), 1);
}

#[test]
fn create_idempotent_errors_when_epic_exists_with_different_spec() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    let spec = specs.make("e");
    let other = specs.make("other");
    let _ = capture(|w| svc.create("e", &spec, None, w));

    let (code, out) = capture(|w| svc.create_with_options("e", &other, None, true, w));
    assert_eq!(code, 1, "stdout: {out}");
    assert!(
        out.contains("different spec_path"),
        "stdout should explain mismatch: {out}"
    );
    // Existing epic untouched.
    let e = store.get_epic("e").unwrap().unwrap();
    assert_eq!(e.spec_path, spec);
}

#[test]
fn create_rejects_missing_spec_file() {
    // F1 wiring lock: `epic create --spec <path>` must reject a path that
    // doesn't exist on disk before any store mutation. Surfaces as a
    // `UserInputError` (anyhow chain) so `main::exit_code_for` maps it to
    // exit code 1.
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let missing = PathBuf::from("/tmp/autopilot-no-such-dir-xyz/spec/missing.md");
    let msg = expect_err(|w| svc.create("e", &missing, None, w));
    assert!(msg.contains("does not exist"), "error: {msg}");
    assert!(msg.contains("missing.md"), "error must name path: {msg}");
    // No epic was inserted.
    assert!(store.list_epics(None).unwrap().is_empty());
}

#[test]
fn list_filters_by_status_and_renders_json() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    let _ = capture(|w| svc.create("a", &specs.make("a"), None, w));
    let _ = capture(|w| svc.create("b", &specs.make("b"), None, w));
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
    let specs = SpecDir::new();
    let _ = capture(|w| svc.create("e", &specs.make("e"), None, w));

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
    let specs = SpecDir::new();
    let _ = capture(|w| svc.create("e", &specs.make("e"), None, w));
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
    let specs = SpecDir::new();
    let spec = specs.make("e");
    let _ = capture(|w| svc.create("e", &spec, None, w));
    let (code, out) = capture(|w| svc.find_by_spec_path(&spec, true, w));
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["name"], "e");
}

#[test]
fn find_by_spec_path_returns_exit_1_when_no_match() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    let none_spec = specs.make("none");
    let (code, _) = capture(|w| svc.find_by_spec_path(&none_spec, false, w));
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
    let (code, out) =
        capture(|w| svc.find_by_spec_path(std::path::Path::new("spec/shared.md"), false, w));
    assert_eq!(code, 3);
    assert!(out.contains("inconsistency"), "stdout: {out}");
}

#[test]
fn reconcile_applies_jsonl_plan_and_is_idempotent() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    let _ = capture(|w| svc.create("e", &specs.make("e"), None, w));

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

#[test]
fn reconcile_rejects_duplicate_task_id_in_plan() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    seed_epic(&svc, &specs, "e");

    let plan = write_plan_jsonl(&[
        r#"{"kind":"task","id":"aaaaaaaaaaaa","title":"first"}"#,
        r#"{"kind":"task","id":"aaaaaaaaaaaa","title":"duplicate"}"#,
    ]);

    let msg = expect_err(|w| svc.reconcile("e", plan.path(), w));
    assert!(
        msg.contains("duplicate task id 'aaaaaaaaaaaa'"),
        "error: {msg}"
    );
    assert!(store.list_tasks_by_epic("e", None).unwrap().is_empty());
}

#[test]
fn reconcile_rejects_unknown_task_source() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    seed_epic(&svc, &specs, "e");

    let plan = write_plan_jsonl(&[
        r#"{"kind":"task","id":"aaaaaaaaaaaa","title":"x","source":"telepathy"}"#,
    ]);
    let msg = expect_err(|w| svc.reconcile("e", plan.path(), w));
    assert!(msg.contains("unknown source 'telepathy'"), "error: {msg}");
    // No partial write on parse error.
    assert!(store.list_tasks_by_epic("e", None).unwrap().is_empty());
}

#[test]
fn reconcile_skips_blank_and_comment_lines() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    seed_epic(&svc, &specs, "e");

    // The `#`-prefixed line is itself valid JSON; if the parser stripped `#`
    // *after* trying to deserialize it would either error or insert task
    // `cccccccccccc`. Asserting only `aaaa...` is created proves `#` is
    // recognized as a comment marker before parse.
    let plan = write_plan_jsonl(&[
        r#"# {"kind":"task","id":"cccccccccccc","title":"comment"}"#,
        "",
        r#"{"kind":"task","id":"aaaaaaaaaaaa","title":"first"}"#,
        "   ",
    ]);
    let (code, _) = capture(|w| svc.reconcile("e", plan.path(), w));
    assert_eq!(code, 0);
    let tasks = store.list_tasks_by_epic("e", None).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id.as_str(), "aaaaaaaaaaaa");
}

#[test]
fn list_renders_human_table_when_not_json() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let specs = SpecDir::new();
    seed_epic(&svc, &specs, "alpha");
    let (code, out) = capture(|w| svc.list(None, false, w));
    assert_eq!(code, 0);
    assert!(out.contains("alpha"));
    assert!(out.contains("epic/alpha"));
    assert!(out.contains("active"));
}

#[test]
fn list_emits_placeholder_when_empty() {
    let (store, clock) = fixture();
    let svc = EpicService::new(store.as_ref(), &clock);
    let (code, out) = capture(|w| svc.list(None, false, w));
    assert_eq!(code, 0);
    assert!(out.contains("(no epics)"));
}
