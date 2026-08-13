//! Black-box tests for the filesystem baseline store: the write contract the
//! session hooks depend on (never clobber, never escape the state dir, never
//! grow unbounded, never expose a half-written file).

mod session_mocks;

use atelier::session::core::baseline::{Baseline, BaselineStore, FsBaselineStore, DEFAULT_TTL};
use session_mocks::{baseline, SESSION};
use std::time::Duration;

fn entries(dir: &std::path::Path) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut names: Vec<String> = read
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn baseline_not_overwritten_on_resume_or_compact() {
    // SessionStart fires again on resume/compact/clear. Re-recording there
    // would move the anchor forward and erase the session's own contribution.
    let tmp = tempfile::TempDir::new().unwrap();
    let store = FsBaselineStore::new(tmp.path(), DEFAULT_TTL);

    let first = baseline("head0", &["src/pre-existing.rs"]);
    assert!(store.save_if_absent(SESSION, &first).unwrap());

    let resumed = baseline("head9", &["src/a.rs", "src/b.rs"]);
    assert!(!store.save_if_absent(SESSION, &resumed).unwrap());
    assert_eq!(store.load(SESSION).unwrap(), first);

    // The notified flag must survive a resume as well, or the banner repeats.
    let mut notified = first.clone();
    notified.mark_notified();
    store.save(SESSION, &notified).unwrap();
    assert!(!store.save_if_absent(SESSION, &resumed).unwrap());
    assert!(store.load(SESSION).unwrap().notified);
}

#[test]
fn rejects_session_id_with_path_separator() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = FsBaselineStore::new(tmp.path().join("state"), DEFAULT_TTL);
    let value = baseline("head0", &["src/lib.rs"]);

    for hostile in ["../../etc/evil", "a/b/c12345", "..", "sess\\win12345"] {
        assert!(
            store.save(hostile, &value).is_err(),
            "{hostile} must be rejected"
        );
        assert!(store.load(hostile).is_none());
    }
    // Too short to be a real session id — rejected by the same guard.
    assert!(store.save("short", &value).is_err());

    // A rejected id must not create the state dir, let alone a file in it.
    assert!(entries(&tmp.path().join("state")).is_empty());
    assert!(store.save(SESSION, &value).is_ok());
}

#[test]
fn prunes_baselines_older_than_ttl() {
    let tmp = tempfile::TempDir::new().unwrap();
    // TTL 0: every file already on disk is expired by the time of the next
    // write, which makes the sweep observable without faking mtimes.
    let expiring = FsBaselineStore::new(tmp.path(), Duration::ZERO);
    let value = baseline("head0", &["src/lib.rs"]);

    expiring.save("sess-stale-01", &value).unwrap();
    expiring.save("sess-fresh-02", &value).unwrap();

    assert!(expiring.load("sess-stale-01").is_none(), "stale swept");
    assert!(
        expiring.load("sess-fresh-02").is_some(),
        "the file being written must survive its own sweep"
    );
    assert_eq!(entries(tmp.path()), vec!["sess-fresh-02.json".to_string()]);

    // Under the real TTL nothing recent is touched.
    let keeping = FsBaselineStore::new(tmp.path(), DEFAULT_TTL);
    keeping.save("sess-third-03", &value).unwrap();
    assert!(keeping.load("sess-fresh-02").is_some());
    assert!(keeping.load("sess-third-03").is_some());
}

#[test]
fn write_is_atomic_under_concurrent_calls() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = FsBaselineStore::new(tmp.path(), DEFAULT_TTL);

    // Distinct, differently sized payloads so a torn write would parse as
    // something that is not one of them (or not parse at all).
    let variants: Vec<Baseline> = (0..8)
        .map(|writer| Baseline {
            head: Some(format!("head{writer}")),
            dirty: (0..=writer)
                .map(|n| format!("src/writer{writer}/file{n}.rs"))
                .collect(),
            notified: writer % 2 == 0,
        })
        .collect();

    std::thread::scope(|scope| {
        for variant in &variants {
            scope.spawn(|| {
                for _ in 0..20 {
                    store.save(SESSION, variant).unwrap();
                }
            });
        }
        for _ in 0..4 {
            scope.spawn(|| {
                for _ in 0..50 {
                    if let Some(loaded) = store.load(SESSION) {
                        assert!(variants.contains(&loaded), "torn read: {loaded:?}");
                    }
                }
            });
        }
    });

    assert!(variants.contains(&store.load(SESSION).unwrap()));
    // Temp files are renamed into place, never left behind.
    assert_eq!(entries(tmp.path()), vec![format!("{SESSION}.json")]);
}
