use std::sync::Arc;

use autopilot::cmd::events::{EventsService, ListArgs};
use autopilot::domain::{Event, EventKind, EventPayload, TaskId};
use autopilot::ports::task_store::TaskStore;
use autopilot::store::InMemoryTaskStore;
use chrono::{DateTime, TimeZone, Utc};

// ---------- helpers ----------

fn base_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
}

type EventSeed<'a> = (
    Option<&'a str>,
    Option<&'a str>,
    EventKind,
    EventPayload,
    i64,
);

fn fixture_with_events() -> Arc<dyn TaskStore> {
    let store: Arc<dyn TaskStore> = Arc::new(InMemoryTaskStore::new());
    let t0 = base_time();
    // (epic, task, kind, payload, +seconds)
    let seeds: Vec<EventSeed> = vec![
        (
            Some("e1"),
            None,
            EventKind::EpicStarted,
            EventPayload::EpicStarted,
            0,
        ),
        (
            Some("e1"),
            Some("aaaaaaaaaaaa"),
            EventKind::TaskInserted,
            EventPayload::TaskInserted {
                source: "spec".into(),
                fingerprint: None,
            },
            10,
        ),
        (
            Some("e1"),
            Some("aaaaaaaaaaaa"),
            EventKind::TaskClaimed,
            EventPayload::TaskClaimed { attempts: 1 },
            20,
        ),
        (
            Some("e1"),
            Some("bbbbbbbbbbbb"),
            EventKind::TaskInserted,
            EventPayload::TaskInserted {
                source: "spec".into(),
                fingerprint: None,
            },
            30,
        ),
        (
            Some("e2"),
            None,
            EventKind::EpicStarted,
            EventPayload::EpicStarted,
            40,
        ),
        (
            Some("e2"),
            Some("cccccccccccc"),
            EventKind::TaskInserted,
            EventPayload::TaskInserted {
                source: "spec".into(),
                fingerprint: None,
            },
            50,
        ),
    ];
    for (epic, task, kind, payload, sec) in seeds {
        let event = Event {
            task_id: task.map(TaskId::from_raw),
            epic_name: epic.map(|s| s.to_string()),
            kind,
            payload,
            at: t0 + chrono::Duration::seconds(sec),
        };
        store.append_event(&event).unwrap();
    }
    store
}

fn capture<F>(f: F) -> (i32, String)
where
    F: FnOnce(&mut Vec<u8>) -> anyhow::Result<i32>,
{
    let mut buf: Vec<u8> = Vec::new();
    let code = f(&mut buf).expect("service call");
    (code, String::from_utf8(buf).expect("utf-8"))
}

fn list_args() -> ListArgs {
    ListArgs {
        epic: None,
        task: None,
        kinds: vec![],
        since: None,
        limit: None,
        json: false,
    }
}

// ---------- list filters ----------

#[test]
fn events_list_filters_by_epic() {
    let store = fixture_with_events();
    let svc = EventsService::new(store.as_ref());
    let args = ListArgs {
        epic: Some("e1".to_string()),
        json: true,
        ..list_args()
    };
    let (code, out) = capture(|w| svc.list(&args, w));
    assert_eq!(code, 0);
    let arr: Vec<serde_json::Value> = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(arr.len(), 4);
    for e in &arr {
        assert_eq!(e["epic"], "e1");
    }
}

#[test]
fn events_list_filters_by_task() {
    let store = fixture_with_events();
    let svc = EventsService::new(store.as_ref());
    let args = ListArgs {
        task: Some("aaaaaaaaaaaa".to_string()),
        json: true,
        ..list_args()
    };
    let (code, out) = capture(|w| svc.list(&args, w));
    assert_eq!(code, 0);
    let arr: Vec<serde_json::Value> = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(arr.len(), 2);
    for e in &arr {
        assert_eq!(e["task"], "aaaaaaaaaaaa");
    }
}

#[test]
fn events_list_filters_by_kind() {
    let store = fixture_with_events();
    let svc = EventsService::new(store.as_ref());
    let args = ListArgs {
        kinds: vec!["task_inserted".to_string()],
        json: true,
        ..list_args()
    };
    let (code, out) = capture(|w| svc.list(&args, w));
    assert_eq!(code, 0);
    let arr: Vec<serde_json::Value> = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(arr.len(), 3);
    for e in &arr {
        assert_eq!(e["kind"], "task_inserted");
    }
}

#[test]
fn events_list_filters_by_since() {
    let store = fixture_with_events();
    let svc = EventsService::new(store.as_ref());
    // Drop the first 3 events (at t+0, t+10s, t+20s) by passing t+25s.
    let args = ListArgs {
        since: Some("2026-01-01T00:00:25Z".to_string()),
        json: true,
        ..list_args()
    };
    let (code, out) = capture(|w| svc.list(&args, w));
    assert_eq!(code, 0);
    let arr: Vec<serde_json::Value> = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(arr.len(), 3);
}

#[test]
fn events_list_respects_limit() {
    let store = fixture_with_events();
    let svc = EventsService::new(store.as_ref());
    let args = ListArgs {
        limit: Some(2),
        json: true,
        ..list_args()
    };
    let (code, out) = capture(|w| svc.list(&args, w));
    assert_eq!(code, 0);
    let arr: Vec<serde_json::Value> = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(arr.len(), 2);
}

#[test]
fn events_list_json_output_is_array_of_event_records() {
    let store = fixture_with_events();
    let svc = EventsService::new(store.as_ref());
    let args = ListArgs {
        json: true,
        ..list_args()
    };
    let (code, out) = capture(|w| svc.list(&args, w));
    assert_eq!(code, 0);
    let arr: Vec<serde_json::Value> = serde_json::from_str(out.trim()).unwrap();
    assert!(!arr.is_empty());
    // Each record must expose at, kind, epic, task, payload.
    for e in &arr {
        assert!(e["at"].is_string(), "missing `at`: {e}");
        assert!(e["kind"].is_string(), "missing `kind`: {e}");
        assert!(e.get("epic").is_some(), "missing `epic` (may be null): {e}");
        assert!(e.get("task").is_some(), "missing `task` (may be null): {e}");
        assert!(e.get("payload").is_some(), "missing `payload`: {e}");
    }
}

#[test]
fn events_list_rejects_unknown_kind() {
    let store = fixture_with_events();
    let svc = EventsService::new(store.as_ref());
    let args = ListArgs {
        kinds: vec!["definitely_not_a_kind".to_string()],
        ..list_args()
    };
    let (code, out) = capture(|w| svc.list(&args, w));
    assert_eq!(code, 1, "stdout: {out}");
    assert!(out.contains("unknown event kind"), "stdout: {out}");
    assert!(out.contains("definitely_not_a_kind"), "stdout: {out}");
}
