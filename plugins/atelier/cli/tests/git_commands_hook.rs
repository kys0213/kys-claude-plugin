//! Port of `git-utils/tests/commands/hook.test.ts` — in-memory FS mock, plus
//! the batch (`register_many`) contract: one write, purge-before-register.

mod git_mocks;

use atelier::git::commands::hook::create_hook_command;
use atelier::git::types::{
    CmdResult, HookListInput, HookRegisterInput, HookRegisterManyInput, HookRegistration,
    HookUnregisterInput,
};
use git_mocks::MockFs;
use serde_json::Value;

const PROJECT_DIR: &str = "/tmp/test-project";
fn settings_path() -> String {
    format!("{PROJECT_DIR}/.claude/settings.json")
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

// ---- register_many (batch) ----

fn registration(hook_type: &str, matcher: &str, command: &str) -> HookRegistration {
    HookRegistration {
        hook_type: hook_type.to_string(),
        matcher: matcher.to_string(),
        command: command.to_string(),
        timeout: None,
    }
}

fn batch(hooks: Vec<HookRegistration>) -> HookRegisterManyInput {
    HookRegisterManyInput {
        hooks,
        remove_command_prefixes: Vec::new(),
        project_dir: Some(PROJECT_DIR.to_string()),
        dry_run: false,
    }
}

#[test]
fn register_many_writes_settings_once() {
    // The reason the batch exists: N separate `register` calls are N
    // read-modify-writes, so a failure midway leaves settings half-registered.
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    hook.register_many(&batch(vec![
        registration("PreToolUse", "Write|Edit", "guard write"),
        registration("PreToolUse", "Bash", "guard commit"),
    ]))
    .unwrap();
    assert_eq!(fs.write_count(), 1);
    let s = settings(&fs);
    assert_eq!(s["hooks"]["PreToolUse"].as_array().unwrap().len(), 2);
}

#[test]
fn register_many_reports_action_per_hook() {
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    hook.register(&reg("PreToolUse", "Bash", "guard commit"))
        .unwrap();
    let r = hook
        .register_many(&batch(vec![
            registration("PreToolUse", "Write|Edit", "guard write"),
            registration("PreToolUse", "Bash", "guard commit"),
        ]))
        .unwrap();
    match r {
        CmdResult::Ok(out) => {
            let actions: Vec<&str> = out.registered.iter().map(|o| o.action.as_str()).collect();
            assert_eq!(actions, vec!["created", "updated"]);
        }
        _ => panic!(),
    }
}

#[test]
fn register_many_purges_prefix_matches_before_registering() {
    // Exact-command replacement cannot retire an entry whose trailing flags
    // differ; the prefix purge is what keeps a stale pin from double-running.
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"guard commit --default-branch main"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    let r = hook
        .register_many(&HookRegisterManyInput {
            hooks: vec![registration("PreToolUse", "Bash", "guard commit")],
            remove_command_prefixes: vec!["guard commit ".to_string()],
            project_dir: Some(PROJECT_DIR.to_string()),
            dry_run: false,
        })
        .unwrap();
    match r {
        CmdResult::Ok(out) => assert_eq!(out.removed, vec!["guard commit --default-branch main"]),
        _ => panic!(),
    }
    let s = settings(&fs);
    let group = s["hooks"]["PreToolUse"][0]["hooks"].as_array().unwrap();
    assert_eq!(group.len(), 1);
    assert_eq!(group[0]["command"], "guard commit");
}

#[test]
fn register_many_purge_does_not_remove_what_it_just_registered() {
    // The registered command shares the purged prefix (re-install case), so
    // purging after registering would delete the fresh entry.
    let fs = MockFs::new();
    let hook = create_hook_command(&fs);
    hook.register_many(&HookRegisterManyInput {
        hooks: vec![registration("PreToolUse", "Bash", "guard commit --x 1")],
        remove_command_prefixes: vec!["guard commit ".to_string()],
        project_dir: Some(PROJECT_DIR.to_string()),
        dry_run: false,
    })
    .unwrap();
    let s = settings(&fs);
    assert_eq!(
        s["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "guard commit --x 1"
    );
}

#[test]
fn register_many_purge_spares_unrelated_commands() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"guard commit --default-branch main"},{"type":"command","command":"protect-stagnation.sh"}]}]}}"#,
    );
    let hook = create_hook_command(&fs);
    hook.register_many(&HookRegisterManyInput {
        hooks: vec![registration("PreToolUse", "Bash", "guard commit")],
        remove_command_prefixes: vec!["guard commit ".to_string()],
        project_dir: Some(PROJECT_DIR.to_string()),
        dry_run: false,
    })
    .unwrap();
    let s = settings(&fs);
    let commands: Vec<&str> = s["hooks"]["PreToolUse"][0]["hooks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["command"].as_str().unwrap())
        .collect();
    assert_eq!(commands, vec!["protect-stagnation.sh", "guard commit"]);
}

#[test]
fn register_many_dry_run_does_not_write() {
    let fs = MockFs::new();
    fs.set(
        &settings_path(),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"guard commit --default-branch main"}]}]}}"#,
    );
    let before = fs.get(&settings_path()).unwrap();
    let hook = create_hook_command(&fs);
    let r = hook
        .register_many(&HookRegisterManyInput {
            hooks: vec![registration("PreToolUse", "Bash", "guard commit")],
            remove_command_prefixes: vec!["guard commit ".to_string()],
            project_dir: Some(PROJECT_DIR.to_string()),
            dry_run: true,
        })
        .unwrap();
    // The plan is still computed and reported...
    match r {
        CmdResult::Ok(out) => {
            assert_eq!(out.removed.len(), 1);
            assert_eq!(out.registered.len(), 1);
        }
        _ => panic!(),
    }
    // ...but nothing reached the filesystem.
    assert_eq!(fs.write_count(), 0);
    assert_eq!(fs.get(&settings_path()).unwrap(), before);
}
