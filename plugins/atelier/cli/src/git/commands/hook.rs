//! `hook` command, ported from git-utils `commands/hook.ts`
//! (originally `register-hook.js`). Registers / unregisters / lists hooks in
//! `<project>/.claude/settings.json`, preserving any other settings keys.
//!
//! Unlike the TS version (which injected an `fs` abstraction), this uses the
//! real filesystem; tests isolate via a temp project directory — matching the
//! crate's other filesystem-touching tests.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::git::types::{
    HookListInput, HookRegisterInput, HookRegisterOutput, HookUnregisterInput, HookUnregisterOutput,
};

fn project_dir(explicit: &Option<String>) -> PathBuf {
    match explicit {
        Some(d) => PathBuf::from(d),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    }
}

fn settings_path(dir: &std::path::Path) -> PathBuf {
    dir.join(".claude").join("settings.json")
}

/// Reads settings as a JSON object, guaranteeing a `"hooks"` object exists.
fn read_settings(dir: &std::path::Path) -> Result<Value, String> {
    let path = settings_path(dir);
    let mut settings: Value = if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())?
    } else {
        json!({ "hooks": {} })
    };
    if !settings.is_object() {
        settings = json!({ "hooks": {} });
    }
    if !settings.get("hooks").map(Value::is_object).unwrap_or(false) {
        settings["hooks"] = json!({});
    }
    Ok(settings)
}

fn write_settings(dir: &std::path::Path, settings: &Value) -> Result<(), String> {
    let claude_dir = dir.join(".claude");
    if !claude_dir.exists() {
        std::fs::create_dir_all(&claude_dir).map_err(|e| e.to_string())?;
    }
    let body = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_path(dir), format!("{body}\n")).map_err(|e| e.to_string())
}

/// `hook register` — adds (or replaces) a matcher under `hook_type`.
pub fn register(input: &HookRegisterInput) -> Result<HookRegisterOutput, String> {
    let dir = project_dir(&input.project_dir);
    let mut settings = read_settings(&dir)?;

    let mut entry = json!({ "type": "command", "command": input.command });
    if let Some(t) = input.timeout {
        entry["timeout"] = json!(t);
    }
    let new_hook = json!({ "matcher": input.matcher, "hooks": [entry] });

    let action;
    {
        let hooks = settings
            .get_mut("hooks")
            .and_then(Value::as_object_mut)
            .expect("hooks ensured object");
        let arr = hooks
            .entry(input.hook_type.clone())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("hook type entry is not an array")?;

        let existing = arr.iter().position(|h| {
            h.get("matcher") == Some(&json!(input.matcher))
                || h.get("hooks").and_then(Value::as_array).is_some_and(|hs| {
                    hs.iter()
                        .any(|x| x.get("command") == Some(&json!(input.command)))
                })
        });

        action = if let Some(i) = existing {
            arr[i] = new_hook;
            "updated"
        } else {
            arr.push(new_hook);
            "created"
        };
    }

    write_settings(&dir, &settings)?;
    Ok(HookRegisterOutput {
        action: action.to_string(),
        command: input.command.clone(),
    })
}

/// `hook unregister` — removes matchers whose hooks invoke `command`.
pub fn unregister(input: &HookUnregisterInput) -> Result<HookUnregisterOutput, String> {
    let dir = project_dir(&input.project_dir);
    if !settings_path(&dir).exists() {
        return Err(format!("No hooks found for type: {}", input.hook_type));
    }
    let mut settings = read_settings(&dir)?;

    let removed = {
        let hooks = settings
            .get_mut("hooks")
            .and_then(Value::as_object_mut)
            .expect("hooks ensured object");
        let Some(arr) = hooks
            .get_mut(&input.hook_type)
            .and_then(Value::as_array_mut)
        else {
            return Err(format!("No hooks found for type: {}", input.hook_type));
        };
        let initial = arr.len();
        arr.retain(|h| {
            !h.get("hooks").and_then(Value::as_array).is_some_and(|hs| {
                hs.iter()
                    .any(|x| x.get("command") == Some(&json!(input.command)))
            })
        });
        if arr.len() == initial {
            return Err(format!("Hook not found: {}", input.command));
        }
        let empty = arr.is_empty();
        if empty {
            hooks.remove(&input.hook_type);
        }
        hooks.is_empty()
    };
    if removed {
        settings.as_object_mut().unwrap().remove("hooks");
    }

    write_settings(&dir, &settings)?;
    Ok(HookUnregisterOutput {
        command: input.command.clone(),
    })
}

/// `hook list` — returns the hooks map, or just one type's entry when filtered.
pub fn list(input: &HookListInput) -> Result<Value, String> {
    let dir = project_dir(&input.project_dir);
    let settings = read_settings(&dir)?;
    let hooks = settings.get("hooks").cloned().unwrap_or_else(|| json!({}));
    match &input.hook_type {
        Some(ht) => {
            let entry = hooks.get(ht).cloned().unwrap_or_else(|| json!([]));
            Ok(json!({ ht.clone(): entry }))
        }
        None => Ok(hooks),
    }
}
