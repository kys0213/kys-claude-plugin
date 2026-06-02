//! `hook` command — port of `git-utils/src/commands/hook.ts`. Manages Claude
//! Code hooks inside `<project>/.claude/settings.json`. The filesystem is
//! abstracted behind `HookFs` so the logic is unit-testable with an in-memory
//! mock, mirroring the TS `deps.fs` injection.
//!
//! Settings are serialized with 2-space indentation and a trailing newline to
//! match the TS `JSON.stringify(settings, null, 2) + '\n'` output.

use crate::git::types::{
    CmdResult, HookListInput, HookRegisterInput, HookRegisterOutput, HookUnregisterInput,
    HookUnregisterOutput,
};
use serde_json::{json, Map, Value};

/// Filesystem operations the hook command depends on. Errors are surfaced as
/// `Err(String)` to mirror the TS promise-rejection paths.
pub trait HookFs {
    fn read_file(&self, path: &str) -> Result<String, String>;
    fn write_file(&self, path: &str, content: &str) -> Result<(), String>;
    fn exists(&self, path: &str) -> bool;
    fn mkdir(&self, path: &str) -> Result<(), String>;
}

pub struct HookCommand<'a> {
    fs: &'a dyn HookFs,
}

/// Constructs the hook command over the given filesystem.
pub fn create_hook_command(fs: &dyn HookFs) -> HookCommand<'_> {
    HookCommand { fs }
}

fn settings_path(project_dir: &str) -> String {
    format!("{project_dir}/.claude/settings.json")
}

fn default_project_dir(explicit: &Option<String>) -> String {
    explicit.clone().unwrap_or_else(|| {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    })
}

/// Serializes settings the same way the TS code does: 2-space indent + newline.
fn serialize_settings(settings: &Value) -> String {
    let mut s = serde_json::to_string_pretty(settings).unwrap_or_else(|_| "{}".to_string());
    s.push('\n');
    s
}

impl HookCommand<'_> {
    /// Reads settings, ensuring `hooks` is present as an object. Returns the
    /// outer `Err` on malformed JSON (so the caller refuses to overwrite).
    fn read_settings(&self, project_dir: &str) -> Result<Value, String> {
        let path = settings_path(project_dir);
        if !self.fs.exists(&path) {
            return Ok(json!({ "hooks": {} }));
        }
        let content = self.fs.read_file(&path)?;
        let mut settings: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        if !settings.is_object() {
            return Err("settings.json is not an object".to_string());
        }
        if !settings
            .get("hooks")
            .map(|h| h.is_object())
            .unwrap_or(false)
        {
            settings["hooks"] = json!({});
        }
        Ok(settings)
    }

    fn write_settings(&self, project_dir: &str, settings: &Value) -> Result<(), String> {
        let claude_dir = format!("{project_dir}/.claude");
        if !self.fs.exists(&claude_dir) {
            self.fs.mkdir(&claude_dir)?;
        }
        self.fs
            .write_file(&settings_path(project_dir), &serialize_settings(settings))
    }

    /// Registers (or updates) a hook entry. See port notes for matching rules.
    pub fn register(
        &self,
        input: &HookRegisterInput,
    ) -> Result<CmdResult<HookRegisterOutput>, String> {
        let project_dir = default_project_dir(&input.project_dir);
        let mut settings = self.read_settings(&project_dir)?;

        let hooks = settings["hooks"].as_object_mut().unwrap();
        let arr = hooks
            .entry(input.hook_type.clone())
            .or_insert_with(|| Value::Array(vec![]));
        let arr = arr.as_array_mut().ok_or("hook type is not an array")?;

        // Build the hook entry.
        let mut hook_entry = Map::new();
        hook_entry.insert("type".to_string(), json!("command"));
        hook_entry.insert("command".to_string(), json!(input.command));
        if let Some(timeout) = input.timeout {
            hook_entry.insert("timeout".to_string(), json!(timeout));
        }
        let new_hook = json!({
            "matcher": input.matcher,
            "hooks": [Value::Object(hook_entry)],
        });

        // Find existing entry by matcher OR by any nested command.
        let existing_index = arr.iter().position(|h| {
            let matcher_match = h.get("matcher").and_then(|m| m.as_str()) == Some(&input.matcher);
            let command_match = h
                .get("hooks")
                .and_then(|hs| hs.as_array())
                .map(|hs| {
                    hs.iter().any(|hk| {
                        hk.get("command").and_then(|c| c.as_str()) == Some(&input.command)
                    })
                })
                .unwrap_or(false);
            matcher_match || command_match
        });

        let action = if let Some(idx) = existing_index {
            arr[idx] = new_hook;
            "updated"
        } else {
            arr.push(new_hook);
            "created"
        };

        self.write_settings(&project_dir, &settings)?;
        Ok(CmdResult::Ok(HookRegisterOutput {
            action: action.to_string(),
            command: input.command.clone(),
        }))
    }

    /// Unregisters a hook by command, pruning empty containers.
    pub fn unregister(
        &self,
        input: &HookUnregisterInput,
    ) -> Result<CmdResult<HookUnregisterOutput>, String> {
        let project_dir = default_project_dir(&input.project_dir);
        let path = settings_path(&project_dir);
        if !self.fs.exists(&path) {
            return Ok(CmdResult::Err(format!(
                "No hooks found for type: {}",
                input.hook_type
            )));
        }

        let mut settings = self.read_settings(&project_dir)?;

        let has_type = settings["hooks"]
            .get(&input.hook_type)
            .map(|v| !v.is_null())
            .unwrap_or(false);
        if !has_type {
            return Ok(CmdResult::Err(format!(
                "No hooks found for type: {}",
                input.hook_type
            )));
        }

        let hooks = settings["hooks"].as_object_mut().unwrap();
        let arr = hooks.get_mut(&input.hook_type).unwrap().as_array().cloned();
        let arr = match arr {
            Some(a) => a,
            None => {
                return Ok(CmdResult::Err(format!(
                    "No hooks found for type: {}",
                    input.hook_type
                )))
            }
        };
        let initial = arr.len();
        let filtered: Vec<Value> = arr
            .into_iter()
            .filter(|h| {
                let has_cmd = h
                    .get("hooks")
                    .and_then(|hs| hs.as_array())
                    .map(|hs| {
                        hs.iter().any(|hk| {
                            hk.get("command").and_then(|c| c.as_str()) == Some(&input.command)
                        })
                    })
                    .unwrap_or(false);
                !has_cmd
            })
            .collect();

        if filtered.len() == initial {
            return Ok(CmdResult::Err(format!("Hook not found: {}", input.command)));
        }

        if filtered.is_empty() {
            hooks.remove(&input.hook_type);
        } else {
            hooks.insert(input.hook_type.clone(), Value::Array(filtered));
        }

        // If hooks object is empty, drop it entirely.
        if settings["hooks"]
            .as_object()
            .map(|o| o.is_empty())
            .unwrap_or(false)
        {
            if let Some(obj) = settings.as_object_mut() {
                obj.remove("hooks");
            }
        }

        self.write_settings(&project_dir, &settings)?;
        Ok(CmdResult::Ok(HookUnregisterOutput {
            command: input.command.clone(),
        }))
    }

    /// Lists hooks, optionally filtered to a single hook type.
    pub fn list(&self, input: &HookListInput) -> Result<CmdResult<Value>, String> {
        let project_dir = default_project_dir(&input.project_dir);
        let settings = self.read_settings(&project_dir)?;

        if let Some(hook_type) = &input.hook_type {
            let hooks = settings["hooks"]
                .get(hook_type)
                .cloned()
                .unwrap_or_else(|| json!([]));
            let mut out = Map::new();
            out.insert(hook_type.clone(), hooks);
            return Ok(CmdResult::Ok(Value::Object(out)));
        }

        let hooks = settings.get("hooks").cloned().unwrap_or_else(|| json!({}));
        Ok(CmdResult::Ok(hooks))
    }
}
