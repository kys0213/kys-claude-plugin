//! `hook` command — port of `git-utils/src/commands/hook.ts`. Manages Claude
//! Code hooks inside `<project>/.claude/settings.json`. The filesystem is
//! abstracted behind `HookFs` so the logic is unit-testable with an in-memory
//! mock, mirroring the TS `deps.fs` injection.
//!
//! Writes are batch-atomic: `register_many` applies a purge plus any number of
//! registrations to one in-memory settings value and writes it once, so a
//! multi-hook install can never land half-registered. `register` is the
//! single-hook case of that batch.
//!
//! Settings are serialized with 2-space indentation and a trailing newline to
//! match the TS `JSON.stringify(settings, null, 2) + '\n'` output.

use crate::git::types::{
    CmdResult, HookListInput, HookRegisterInput, HookRegisterManyInput, HookRegisterManyOutput,
    HookRegisterOutput, HookRegistration, HookUnregisterInput, HookUnregisterOutput,
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

/// Settings file a scope directory maps to. Public so callers that report the
/// file they touched (e.g. `setup guard`) do not re-derive the layout.
pub fn settings_path(project_dir: &str) -> String {
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

/// Removes every hook whose command satisfies `matches` from each matcher entry
/// of a hook-type array, pruning entries whose `hooks` list becomes empty.
/// Returns the removed command strings. Sibling commands sharing a matcher
/// group are left untouched.
fn remove_where(arr: &mut Vec<Value>, matches: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut removed = Vec::new();
    for entry in arr.iter_mut() {
        if let Some(hs) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) {
            hs.retain(|hk| match hk.get("command").and_then(|c| c.as_str()) {
                Some(cmd) if matches(cmd) => {
                    removed.push(cmd.to_string());
                    false
                }
                _ => true,
            });
        }
    }
    arr.retain(|entry| {
        entry
            .get("hooks")
            .and_then(|h| h.as_array())
            .map(|h| !h.is_empty())
            .unwrap_or(true)
    });
    removed
}

/// Exact-command removal — the identity rule `register`/`unregister` use.
fn remove_command(arr: &mut Vec<Value>, command: &str) -> bool {
    !remove_where(arr, &|c| c == command).is_empty()
}

/// Drops every hook whose command starts with one of `prefixes`, across all
/// hook types, pruning hook types left empty. Returns the removed commands.
fn purge_prefixes(settings: &mut Value, prefixes: &[String]) -> Vec<String> {
    if prefixes.is_empty() {
        return Vec::new();
    }
    let hooks = match settings["hooks"].as_object_mut() {
        Some(h) => h,
        None => return Vec::new(),
    };
    let mut removed = Vec::new();
    let mut emptied = Vec::new();
    for (hook_type, value) in hooks.iter_mut() {
        if let Some(arr) = value.as_array_mut() {
            removed.extend(remove_where(arr, &|cmd| {
                prefixes.iter().any(|p| cmd.starts_with(p.as_str()))
            }));
            if arr.is_empty() {
                emptied.push(hook_type.clone());
            }
        }
    }
    for hook_type in emptied {
        hooks.remove(&hook_type);
    }
    removed
}

/// Applies one registration to an in-memory settings value. Identity is the
/// command string: any prior registration of the same command (under any
/// matcher) is removed, then the command is appended to the matcher group — so
/// several commands can share one matcher (e.g. multiple PreToolUse/Bash
/// guards, #772) and re-registering is idempotent. No I/O happens here; the
/// batch writes once after every registration has been applied.
fn apply_registration(
    settings: &mut Value,
    reg: &HookRegistration,
) -> Result<HookRegisterOutput, String> {
    let hooks = settings["hooks"]
        .as_object_mut()
        .ok_or("hooks is not an object")?;
    let arr = hooks
        .entry(reg.hook_type.clone())
        .or_insert_with(|| Value::Array(vec![]));
    let arr = arr.as_array_mut().ok_or("hook type is not an array")?;

    let existed = remove_command(arr, &reg.command);

    // Build the hook object.
    let mut hook_entry = Map::new();
    hook_entry.insert("type".to_string(), json!("command"));
    hook_entry.insert("command".to_string(), json!(reg.command));
    if let Some(timeout) = reg.timeout {
        hook_entry.insert("timeout".to_string(), json!(timeout));
    }

    // Append to the entry with the same matcher, creating it if absent.
    let group = arr
        .iter_mut()
        .find(|h| h.get("matcher").and_then(|m| m.as_str()) == Some(&reg.matcher));
    match group {
        Some(entry) => {
            entry["hooks"]
                .as_array_mut()
                .ok_or("hooks is not an array")?
                .push(Value::Object(hook_entry));
        }
        None => {
            arr.push(json!({
                "matcher": reg.matcher,
                "hooks": [Value::Object(hook_entry)],
            }));
        }
    }

    Ok(HookRegisterOutput {
        action: if existed { "updated" } else { "created" }.to_string(),
        command: reg.command.clone(),
    })
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

    /// Applies a batch — prefix purge first, then the registrations — against a
    /// single read and a single write. Ordering matters: a registration whose
    /// command shares the purged prefix (the common re-install case) must
    /// survive, so the purge runs before anything is added.
    pub fn register_many(
        &self,
        input: &HookRegisterManyInput,
    ) -> Result<CmdResult<HookRegisterManyOutput>, String> {
        let project_dir = default_project_dir(&input.project_dir);
        let mut settings = self.read_settings(&project_dir)?;

        let removed = purge_prefixes(&mut settings, &input.remove_command_prefixes);

        let mut registered = Vec::with_capacity(input.hooks.len());
        for reg in &input.hooks {
            registered.push(apply_registration(&mut settings, reg)?);
        }

        if !input.dry_run {
            self.write_settings(&project_dir, &settings)?;
        }
        Ok(CmdResult::Ok(HookRegisterManyOutput {
            registered,
            removed,
        }))
    }

    /// Registers (or updates) a single hook command — the one-element batch.
    pub fn register(
        &self,
        input: &HookRegisterInput,
    ) -> Result<CmdResult<HookRegisterOutput>, String> {
        let batch = HookRegisterManyInput {
            hooks: vec![HookRegistration {
                hook_type: input.hook_type.clone(),
                matcher: input.matcher.clone(),
                command: input.command.clone(),
                timeout: input.timeout,
            }],
            remove_command_prefixes: Vec::new(),
            project_dir: input.project_dir.clone(),
            dry_run: false,
        };
        Ok(match self.register_many(&batch)? {
            CmdResult::Ok(out) => match out.registered.into_iter().next() {
                Some(one) => CmdResult::Ok(one),
                None => CmdResult::Err("registration produced no result".to_string()),
            },
            CmdResult::Err(e) => CmdResult::Err(e),
        })
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

        let hooks = settings["hooks"].as_object_mut().unwrap();
        let arr = match hooks
            .get_mut(&input.hook_type)
            .and_then(|v| v.as_array_mut())
        {
            Some(a) => a,
            None => {
                return Ok(CmdResult::Err(format!(
                    "No hooks found for type: {}",
                    input.hook_type
                )))
            }
        };

        if !remove_command(arr, &input.command) {
            return Ok(CmdResult::Err(format!("Hook not found: {}", input.command)));
        }

        if arr.is_empty() {
            hooks.remove(&input.hook_type);
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
