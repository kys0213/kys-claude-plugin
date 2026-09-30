use assert_cmd::Command;
use atelier::session::commands::ensure_env::{EnsureEnvCommand, EnsureEnvOutcome};
use atelier::session::core::settings_env::{ensure_env_key, EnvEdit, FsSettingsFile, SettingsFile};
use serde_json::Value;
use std::cell::RefCell;

const KEY: &str = "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS";

fn parse(content: &str) -> Value {
    serde_json::from_str(content).expect("valid JSON")
}

fn added(edit: EnvEdit) -> String {
    match edit {
        EnvEdit::Add(content) => content,
        other => panic!("expected Add, got {other:?}"),
    }
}

#[test]
fn missing_file_becomes_env_only_settings() {
    let content = added(ensure_env_key(None, KEY, "1"));
    assert_eq!(parse(&content), serde_json::json!({ "env": { KEY: "1" } }));
    assert!(content.ends_with('\n'));
}

#[test]
fn other_top_level_keys_survive_in_their_order() {
    let existing = r#"{
  "model": "opus",
  "hooks": { "Stop": [] },
  "permissions": { "allow": ["Bash(ls)"] }
}
"#;
    let content = added(ensure_env_key(Some(existing), KEY, "1"));
    let v = parse(&content);
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["model", "hooks", "permissions", "env"]);
    assert_eq!(v["permissions"]["allow"][0], "Bash(ls)");
    assert_eq!(v["env"][KEY], "1");
}

#[test]
fn existing_env_vars_are_kept() {
    let existing = r#"{ "env": { "FOO": "bar" } }"#;
    let v = parse(&added(ensure_env_key(Some(existing), KEY, "1")));
    assert_eq!(v["env"]["FOO"], "bar");
    assert_eq!(v["env"][KEY], "1");
}

#[test]
fn a_value_the_user_set_is_never_overwritten() {
    for value in [r#""0""#, r#""1""#, r#""""#, "1", "false"] {
        let existing = format!(r#"{{ "env": {{ "{KEY}": {value} }} }}"#);
        match ensure_env_key(Some(&existing), KEY, "1") {
            EnvEdit::Present(v) => assert_eq!(v, parse(value)),
            other => panic!("value {value}: expected Present, got {other:?}"),
        }
    }
}

#[test]
fn unparseable_settings_are_left_alone() {
    for existing in [
        "{ not json",
        "[]",
        r#""text""#,
        r#"{ "env": [] }"#,
        r#"{ "env": "x" }"#,
    ] {
        assert!(
            matches!(
                ensure_env_key(Some(existing), KEY, "1"),
                EnvEdit::Unusable(_)
            ),
            "{existing} should be Unusable"
        );
    }
}

#[test]
fn empty_file_is_treated_as_missing() {
    let v = parse(&added(ensure_env_key(Some("  \n"), KEY, "1")));
    assert_eq!(v, serde_json::json!({ "env": { KEY: "1" } }));
}

#[derive(Default)]
struct MemFile {
    content: RefCell<Option<String>>,
    writes: RefCell<usize>,
    fail_write: bool,
}

impl SettingsFile for MemFile {
    fn read(&self) -> Result<Option<String>, String> {
        Ok(self.content.borrow().clone())
    }
    fn replace(&self, content: &str) -> Result<Option<String>, String> {
        if self.fail_write {
            return Err("read-only".to_string());
        }
        *self.writes.borrow_mut() += 1;
        let backup = self
            .content
            .borrow()
            .as_ref()
            .map(|_| "mem.bak".to_string());
        *self.content.borrow_mut() = Some(content.to_string());
        Ok(backup)
    }
}

#[test]
fn second_run_is_a_no_op() {
    let file = MemFile::default();
    assert_eq!(
        EnsureEnvCommand::new(&file).run(KEY, "1"),
        EnsureEnvOutcome::Added { backup: None }
    );
    assert_eq!(
        EnsureEnvCommand::new(&file).run(KEY, "1"),
        EnsureEnvOutcome::AlreadySet
    );
    assert_eq!(*file.writes.borrow(), 1);
}

#[test]
fn backup_is_reported_when_a_file_existed() {
    let file = MemFile {
        content: RefCell::new(Some("{}".to_string())),
        ..Default::default()
    };
    assert_eq!(
        EnsureEnvCommand::new(&file).run(KEY, "1"),
        EnsureEnvOutcome::Added {
            backup: Some("mem.bak".to_string())
        }
    );
}

#[test]
fn unusable_or_unwritable_settings_are_skipped_with_a_reason() {
    let broken = MemFile {
        content: RefCell::new(Some("{".to_string())),
        ..Default::default()
    };
    assert!(matches!(
        EnsureEnvCommand::new(&broken).run(KEY, "1"),
        EnsureEnvOutcome::Skipped(_)
    ));
    assert_eq!(*broken.writes.borrow(), 0);

    let read_only = MemFile {
        fail_write: true,
        ..Default::default()
    };
    assert!(matches!(
        EnsureEnvCommand::new(&read_only).run(KEY, "1"),
        EnsureEnvOutcome::Skipped(_)
    ));
}

fn names(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    v.sort();
    v
}

#[test]
fn fs_creates_missing_parent_and_leaves_no_temp_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path().join(".claude");
    let file = FsSettingsFile::new(dir.join("settings.json"));

    assert_eq!(file.read().unwrap(), None);
    assert_eq!(file.replace("{}\n").unwrap(), None);
    assert_eq!(names(&dir), ["settings.json"]);
}

#[test]
fn fs_backs_up_the_previous_file_before_replacing() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, "{\"model\":\"opus\"}\n").unwrap();
    let file = FsSettingsFile::new(path.clone());

    let backup = file.replace("{}\n").unwrap().expect("backup path");
    assert_eq!(
        std::fs::read_to_string(&backup).unwrap(),
        "{\"model\":\"opus\"}\n"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}\n");
    assert_eq!(names(tmp.path()).len(), 2, "settings + one backup only");
}

#[cfg(unix)]
#[test]
fn fs_writes_through_a_symlinked_settings_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let dotfiles = tmp.path().join("dotfiles");
    std::fs::create_dir(&dotfiles).unwrap();
    let real = dotfiles.join("settings.json");
    std::fs::write(&real, "{}\n").unwrap();
    let claude = tmp.path().join(".claude");
    std::fs::create_dir(&claude).unwrap();
    let link = claude.join("settings.json");
    std::os::unix::fs::symlink(&real, &link).unwrap();

    FsSettingsFile::new(link.clone())
        .replace("{\"env\":{}}\n")
        .unwrap();

    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(std::fs::read_to_string(&real).unwrap(), "{\"env\":{}}\n");
    assert_eq!(names(&claude), ["settings.json"]);
}

#[cfg(unix)]
#[test]
fn fs_keeps_the_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, "{}\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    FsSettingsFile::new(path.clone()).replace("{}\n").unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

fn atelier() -> Command {
    Command::cargo_bin("atelier").expect("locate `atelier` cargo binary")
}

#[test]
fn cli_adds_once_then_stays_silent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    let path_arg = path.to_str().unwrap();
    let args = [
        "session",
        "ensure-env",
        "--settings",
        path_arg,
        "--key",
        KEY,
        "--value",
        "1",
    ];

    let first = atelier().args(args).write_stdin("{}").assert().success();
    let out = String::from_utf8(first.get_output().stdout.clone()).unwrap();
    assert!(out.contains(KEY), "announces the added key: {out}");
    assert_eq!(
        parse(&std::fs::read_to_string(&path).unwrap())["env"][KEY],
        "1"
    );

    atelier()
        .args(args)
        .write_stdin("{}")
        .assert()
        .success()
        .stdout("");
}

#[test]
fn cli_reports_but_does_not_touch_broken_settings() {
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("settings.json");
    std::fs::write(&path, "{ broken").unwrap();

    let assert = atelier()
        .args([
            "session",
            "ensure-env",
            "--settings",
            path.to_str().unwrap(),
        ])
        .args(["--key", KEY, "--value", "1"])
        .write_stdin("{}")
        .assert()
        .success();
    let out = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert!(!out.is_empty(), "explains why nothing was added");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken");
}

#[test]
fn cli_without_required_args_still_exits_zero() {
    atelier()
        .args(["session", "ensure-env", "--key", KEY])
        .write_stdin("{}")
        .assert()
        .success();
}
