//! Port of `git-utils/tests/commands/hook.test.ts` — in-memory FS mock.

use atelier::git::commands::hook::{create_hook_command, HookFs};
use atelier::git::types::{CmdResult, HookListInput, HookRegisterInput, HookUnregisterInput};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;

const PROJECT_DIR: &str = "/tmp/test-project";
fn settings_path() -> String {
    format!("{PROJECT_DIR}/.claude/settings.json")
}

/// In-memory FS matching the TS `createMockFs`: `exists` returns true for a key
/// or any key under `<path>/`.
struct MockFs {
    files: RefCell<HashMap<String, String>>,
}

impl MockFs {
    fn new() -> Self {
        MockFs {
            files: RefCell::new(HashMap::new()),
        }
    }
    fn set(&self, path: &str, content: &str) {
        self.files
            .borrow_mut()
            .insert(path.to_string(), content.to_string());
    }
    fn get(&self, path: &str) -> Option<String> {
        self.files.borrow().get(path).cloned()
    }
}

impl HookFs for MockFs {
    fn read_file(&self, path: &str) -> Result<String, String> {
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| format!("File not found: {path}"))
    }
    fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        self.files
            .borrow_mut()
            .insert(path.to_string(), content.to_string());
        Ok(())
    }
    fn exists(&self, path: &str) -> bool {
        let files = self.files.borrow();
        if files.contains_key(path) {
            return true;
        }
        let prefix = format!("{path}/");
        files.keys().any(|k| k.starts_with(&prefix))
    }
    fn mkdir(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }
}

fn reg(hook_type: &str, matcher: &str, command: &str) -> HookRegisterInput {
    HookRegisterInput {
        hook_type: hook_type.to_string(),
        matcher: matcher.to_string(),
        command: command.to_string(),
        timeout: None,
        project_dir: Some(PROJECT_DIR.to_string()),
    }
}

fn settings(fs: &MockFs) -> Value {
    serde_json::from_str(&fs.get(&settings_path()).unwrap()).unwrap()
}

// ---- register ----

#[test]
fn register_creates_settings_when_missing() {
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    let r = hook.register(&reg("Stop", "*", "bash hook.sh")).unwrap();
    assert!(r.is_ok());
    assert!(fs.get(&settings_path()).is_some());
}

#[test]
fn register_adds_to_empty_hooks() {
    let fs = MockFs::new();
    fs.set(&settings_path(), r#"{"hooks":{}}"#);
    let hook = create_hook_command(&fs);
    hook.register(&reg("Stop", "*", "bash hook.sh")).unwrap();
    assert_eq!(settings(&fs)["hooks"]["Stop"].as_array().unwrap().len(), 1);
}

#[test]
fn register_same_command_updates() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"Stop":[{"matcher":"*","hooks":[{"type":"command","command":"bash hook.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    let r = hook
        .register(&reg("Stop", "Write|Edit", "bash hook.sh"))
        .unwrap();
    match r {
        CmdResult::Ok(d) => assert_eq!(d.action, "updated"),
        _ => panic!(),
    }
    let s = settings(&fs);
    assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 1);
    assert_eq!(s["hooks"]["Stop"][0]["matcher"], "Write|Edit");
}

#[test]
fn register_same_matcher_different_command_appends_to_group() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"Stop":[{"matcher":"*","hooks":[{"type":"command","command":"bash old.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    let r = hook.register(&reg("Stop", "*", "bash new.sh")).unwrap();
    match r {
        CmdResult::Ok(d) => assert_eq!(d.action, "created"),
        _ => panic!(),
    }
    let s = settings(&fs);
    assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 1);
    let group = s["hooks"]["Stop"][0]["hooks"].as_array().unwrap();
    assert_eq!(group.len(), 2);
    assert_eq!(group[0]["command"], "bash old.sh");
    assert_eq!(group[1]["command"], "bash new.sh");
}

#[test]
fn register_same_matcher_multiple_commands_coexist() {
    // setup scenario (#772): PreToolUse/Bash holds commit guard + autopilot
    // hooks side by side — registering one must not clobber the others.
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    hook.register(&reg("PreToolUse", "Bash", "atelier git guard commit"))
        .unwrap();
    hook.register(&reg("PreToolUse", "Bash", "guard-pr-base.sh"))
        .unwrap();
    hook.register(&reg("PreToolUse", "Bash", "protect-stagnation.sh"))
        .unwrap();
    let s = settings(&fs);
    assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    let group = s["hooks"]["PreToolUse"][0]["hooks"].as_array().unwrap();
    let commands: Vec<&str> = group
        .iter()
        .map(|h| h["command"].as_str().unwrap())
        .collect();
    assert_eq!(
        commands,
        vec![
            "atelier git guard commit",
            "guard-pr-base.sh",
            "protect-stagnation.sh"
        ]
    );
}

#[test]
fn register_is_idempotent_for_same_matcher_and_command() {
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    hook.register(&reg("PreToolUse", "Bash", "atelier git guard commit"))
        .unwrap();
    let r = hook
        .register(&reg("PreToolUse", "Bash", "atelier git guard commit"))
        .unwrap();
    match r {
        CmdResult::Ok(d) => assert_eq!(d.action, "updated"),
        _ => panic!(),
    }
    let s = settings(&fs);
    assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);
    assert_eq!(
        s["hooks"]["PreToolUse"][0]["hooks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn register_different_matcher_and_command_creates() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"Stop":[{"matcher":"*","hooks":[{"type":"command","command":"bash old.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    let r = hook
        .register(&reg("Stop", "Write|Edit", "bash new.sh"))
        .unwrap();
    match r {
        CmdResult::Ok(d) => assert_eq!(d.action, "created"),
        _ => panic!(),
    }
    assert_eq!(settings(&fs)["hooks"]["Stop"].as_array().unwrap().len(), 2);
}

#[test]
fn register_timeout_included() {
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    let mut input = reg("Stop", "*", "bash hook.sh");
    input.timeout = Some(10);
    hook.register(&input).unwrap();
    assert_eq!(settings(&fs)["hooks"]["Stop"][0]["hooks"][0]["timeout"], 10);
}

#[test]
fn register_no_timeout_omitted() {
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    hook.register(&reg("Stop", "*", "bash hook.sh")).unwrap();
    assert!(settings(&fs)["hooks"]["Stop"][0]["hooks"][0]["timeout"].is_null());
}

// ---- unregister ----

#[test]
fn unregister_existing_succeeds() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"Stop":[{"matcher":"*","hooks":[{"type":"command","command":"bash hook.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    let r = hook
        .unregister(&HookUnregisterInput {
            hook_type: "Stop".to_string(),
            command: "bash hook.sh".to_string(),
            project_dir: Some(PROJECT_DIR.to_string()),
        })
        .unwrap();
    assert!(r.is_ok());
}

#[test]
fn unregister_missing_hook_fails() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"Stop":[{"matcher":"*","hooks":[{"type":"command","command":"bash other.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    let r = hook
        .unregister(&HookUnregisterInput {
            hook_type: "Stop".to_string(),
            command: "bash hook.sh".to_string(),
            project_dir: Some(PROJECT_DIR.to_string()),
        })
        .unwrap();
    match r {
        CmdResult::Err(e) => assert!(e.contains("not found")),
        _ => panic!("expected err"),
    }
}

#[test]
fn unregister_keeps_sibling_hooks_in_matcher_group() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"atelier git guard commit"},{"type":"command","command":"guard-pr-base.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    let r = hook
        .unregister(&HookUnregisterInput {
            hook_type: "PreToolUse".to_string(),
            command: "guard-pr-base.sh".to_string(),
            project_dir: Some(PROJECT_DIR.to_string()),
        })
        .unwrap();
    assert!(r.is_ok());
    let s = settings(&fs);
    let group = s["hooks"]["PreToolUse"][0]["hooks"].as_array().unwrap();
    assert_eq!(group.len(), 1);
    assert_eq!(group[0]["command"], "atelier git guard commit");
}

#[test]
fn unregister_empties_drop_hooks_key() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"Stop":[{"matcher":"*","hooks":[{"type":"command","command":"bash hook.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    hook.unregister(&HookUnregisterInput {
        hook_type: "Stop".to_string(),
        command: "bash hook.sh".to_string(),
        project_dir: Some(PROJECT_DIR.to_string()),
    })
    .unwrap();
    assert!(settings(&fs).get("hooks").is_none());
}

#[test]
fn unregister_no_settings_fails() {
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    let r = hook
        .unregister(&HookUnregisterInput {
            hook_type: "Stop".to_string(),
            command: "bash hook.sh".to_string(),
            project_dir: Some(PROJECT_DIR.to_string()),
        })
        .unwrap();
    assert!(!r.is_ok());
}

// ---- list ----

#[test]
fn list_specific_type() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"Stop":[{"matcher":"*","hooks":[{"type":"command","command":"bash stop.sh"}]}],"PreToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":"bash pre.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    let r = hook
        .list(&HookListInput {
            hook_type: Some("Stop".to_string()),
            project_dir: Some(PROJECT_DIR.to_string()),
        })
        .unwrap();
    match r {
        CmdResult::Ok(v) => {
            let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
            assert_eq!(keys, vec!["Stop"]);
            assert_eq!(v["Stop"].as_array().unwrap().len(), 1);
        }
        _ => panic!(),
    }
}

#[test]
fn list_all() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"Stop":[{"matcher":"*","hooks":[{"type":"command","command":"bash stop.sh"}]}],"PreToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":"bash pre.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    let r = hook
        .list(&HookListInput {
            hook_type: None,
            project_dir: Some(PROJECT_DIR.to_string()),
        })
        .unwrap();
    match r {
        CmdResult::Ok(v) => assert_eq!(v.as_object().unwrap().len(), 2),
        _ => panic!(),
    }
}

#[test]
fn list_empty() {
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    let r = hook
        .list(&HookListInput {
            hook_type: None,
            project_dir: Some(PROJECT_DIR.to_string()),
        })
        .unwrap();
    match r {
        CmdResult::Ok(v) => assert_eq!(v.as_object().unwrap().len(), 0),
        _ => panic!(),
    }
}

// ---- integrity ----

#[test]
fn preserves_other_fields() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"customField":"preserved","hooks":{}}"#,
    );
    let hook = create_hook_command(&fs);
    hook.register(&reg("Stop", "*", "bash hook.sh")).unwrap();
    assert_eq!(settings(&fs)["customField"], "preserved");
}

#[test]
fn json_format_indent_and_newline() {
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    hook.register(&reg("Stop", "*", "bash hook.sh")).unwrap();
    let raw = fs.get(&settings_path()).unwrap();
    assert!(raw.contains("  "));
    assert!(raw.ends_with('\n'));
}

#[test]
fn broken_json_errors_without_overwrite() {
    let fs = MockFs::new();
    fs.set(&settings_path(), "{broken json");
    let hook = create_hook_command(&fs);
    let result = hook.register(&reg("Stop", "*", "bash hook.sh"));
    assert!(result.is_err());
    // Original content untouched.
    assert_eq!(fs.get(&settings_path()).unwrap(), "{broken json");
}
