//! Behaviour port of git-utils `tests/commands/hook.test.ts`, isolated via a
//! temp project directory instead of a mock fs.

use serde_json::{json, Value};
use tempfile::TempDir;

use atelier::git::commands::hook;
use atelier::git::types::{HookListInput, HookRegisterInput, HookUnregisterInput};

fn dir_of(t: &TempDir) -> Option<String> {
    Some(t.path().to_str().unwrap().to_string())
}

fn settings(t: &TempDir) -> Value {
    let path = t.path().join(".claude").join("settings.json");
    let content = std::fs::read_to_string(path).expect("settings.json exists");
    serde_json::from_str(&content).expect("valid json")
}

fn reg(t: &TempDir, hook_type: &str, matcher: &str, command: &str) -> HookRegisterInput {
    HookRegisterInput {
        hook_type: hook_type.to_string(),
        matcher: matcher.to_string(),
        command: command.to_string(),
        timeout: None,
        project_dir: dir_of(t),
    }
}

// ---------- register ----------

#[test]
fn register_creates_settings_and_entry() {
    let t = TempDir::new().unwrap();
    let out = hook::register(&reg(&t, "PreToolUse", "Bash", "guard.sh")).unwrap();
    assert_eq!(out.action, "created");
    let s = settings(&t);
    assert_eq!(s["hooks"]["PreToolUse"][0]["matcher"], json!("Bash"));
    assert_eq!(
        s["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        json!("guard.sh")
    );
}

#[test]
fn register_includes_timeout_when_set() {
    let t = TempDir::new().unwrap();
    hook::register(&HookRegisterInput {
        timeout: Some(30),
        ..reg(&t, "PreToolUse", "Bash", "guard.sh")
    })
    .unwrap();
    assert_eq!(
        settings(&t)["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"],
        json!(30)
    );
}

#[test]
fn register_updates_existing_matcher() {
    let t = TempDir::new().unwrap();
    hook::register(&reg(&t, "PreToolUse", "Bash", "old.sh")).unwrap();
    let out = hook::register(&reg(&t, "PreToolUse", "Bash", "new.sh")).unwrap();
    assert_eq!(out.action, "updated");
    let s = settings(&t);
    assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    assert_eq!(
        s["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        json!("new.sh")
    );
}

#[test]
fn register_dedups_by_command() {
    let t = TempDir::new().unwrap();
    hook::register(&reg(&t, "PreToolUse", "Bash", "same.sh")).unwrap();
    let out = hook::register(&reg(&t, "PreToolUse", "Write", "same.sh")).unwrap();
    assert_eq!(out.action, "updated");
    assert_eq!(
        settings(&t)["hooks"]["PreToolUse"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn register_preserves_other_settings_keys() {
    let t = TempDir::new().unwrap();
    std::fs::create_dir_all(t.path().join(".claude")).unwrap();
    std::fs::write(
        t.path().join(".claude").join("settings.json"),
        r#"{"model":"opus","hooks":{}}"#,
    )
    .unwrap();
    hook::register(&reg(&t, "PreToolUse", "Bash", "guard.sh")).unwrap();
    assert_eq!(settings(&t)["model"], json!("opus"));
}

// ---------- unregister ----------

#[test]
fn unregister_removes_hook() {
    let t = TempDir::new().unwrap();
    hook::register(&reg(&t, "PreToolUse", "Bash", "guard.sh")).unwrap();
    let out = hook::unregister(&HookUnregisterInput {
        hook_type: "PreToolUse".to_string(),
        command: "guard.sh".to_string(),
        project_dir: dir_of(&t),
    })
    .unwrap();
    assert_eq!(out.command, "guard.sh");
    // empty type key removed, then empty hooks object removed
    assert!(settings(&t).get("hooks").is_none());
}

#[test]
fn unregister_no_settings_file_errors() {
    let t = TempDir::new().unwrap();
    let err = hook::unregister(&HookUnregisterInput {
        hook_type: "PreToolUse".to_string(),
        command: "guard.sh".to_string(),
        project_dir: dir_of(&t),
    })
    .unwrap_err();
    assert!(err.contains("No hooks found for type"));
}

#[test]
fn unregister_missing_command_errors() {
    let t = TempDir::new().unwrap();
    hook::register(&reg(&t, "PreToolUse", "Bash", "guard.sh")).unwrap();
    let err = hook::unregister(&HookUnregisterInput {
        hook_type: "PreToolUse".to_string(),
        command: "other.sh".to_string(),
        project_dir: dir_of(&t),
    })
    .unwrap_err();
    assert!(err.contains("Hook not found"));
}

// ---------- list ----------

#[test]
fn list_all_hooks() {
    let t = TempDir::new().unwrap();
    hook::register(&reg(&t, "PreToolUse", "Bash", "guard.sh")).unwrap();
    let listed = hook::list(&HookListInput {
        hook_type: None,
        project_dir: dir_of(&t),
    })
    .unwrap();
    assert!(listed["PreToolUse"].is_array());
}

#[test]
fn list_filtered_by_type() {
    let t = TempDir::new().unwrap();
    hook::register(&reg(&t, "PreToolUse", "Bash", "guard.sh")).unwrap();
    let listed = hook::list(&HookListInput {
        hook_type: Some("SessionStart".to_string()),
        project_dir: dir_of(&t),
    })
    .unwrap();
    // requested type absent → empty array under that key
    assert_eq!(listed["SessionStart"], json!([]));
}

#[test]
fn list_empty_when_no_settings() {
    let t = TempDir::new().unwrap();
    let listed = hook::list(&HookListInput {
        hook_type: None,
        project_dir: dir_of(&t),
    })
    .unwrap();
    assert_eq!(listed, json!({}));
}
