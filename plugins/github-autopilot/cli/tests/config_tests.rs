use std::path::PathBuf;

use autopilot::config::Config;
use tempfile::NamedTempFile;

// ---------- helpers ----------

fn write_toml(content: &str) -> NamedTempFile {
    let f = NamedTempFile::new().unwrap();
    std::fs::write(f.path(), content).unwrap();
    f
}

// ---------- defaults ----------

#[test]
fn config_loads_defaults_when_file_missing() {
    // Pick a path that does not exist; load() must not error and must return defaults.
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_path_buf();
    drop(tmp); // closes and removes the file
    assert!(!path.exists(), "precondition: path must be missing");

    let cfg = Config::load(&path).expect("load() with missing file should be Ok");
    assert_eq!(cfg, Config::default());
    assert_eq!(
        cfg.storage.db_path,
        PathBuf::from(".autopilot/state.db"),
        "default db_path"
    );
    assert_eq!(
        cfg.epic.default_max_attempts, 3,
        "default max_attempts matches PR-B's previous constant"
    );
}

// ---------- field overrides ----------

#[test]
fn config_loads_storage_db_path_from_toml() {
    let f = write_toml("[storage]\ndb_path = \"/tmp/custom.db\"\n");
    let cfg = Config::load(f.path()).expect("load()");
    assert_eq!(cfg.storage.db_path, PathBuf::from("/tmp/custom.db"));
    // Other sections still get defaults.
    assert_eq!(cfg.epic.default_max_attempts, 3);
}

#[test]
fn config_loads_epic_max_attempts_from_toml() {
    let f = write_toml("[epic]\ndefault_max_attempts = 7\n");
    let cfg = Config::load(f.path()).expect("load()");
    assert_eq!(cfg.epic.default_max_attempts, 7);
    // Storage default still applies.
    assert_eq!(cfg.storage.db_path, PathBuf::from(".autopilot/state.db"));
}

// ---------- failure modes ----------

#[test]
fn config_load_returns_error_on_invalid_toml() {
    let f = write_toml("this is = not = valid toml [[\n");
    let err = Config::load(f.path()).unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("parsing config"), "error: {msg}");
}
