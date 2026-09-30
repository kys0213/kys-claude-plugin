//! Adds one key to the `env` object of a Claude Code settings file. It only
//! ever adds: a key already present keeps its value whatever it is, and a file
//! that does not parse as a settings object is never rewritten.

use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// What `ensure_env_key` decided for a settings document.
#[derive(Debug, PartialEq)]
pub enum EnvEdit {
    /// The key was absent; this is the full new file content.
    Add(String),
    /// The key is already set, to this value. Nothing to write.
    Present(Value),
    /// The document cannot be edited safely (not JSON, not an object, or an
    /// `env` that is not an object). Nothing to write.
    Unusable(String),
}

/// Decides the edit for `existing` (the file content, `None` when the file is
/// absent). An empty or whitespace-only file counts as absent.
pub fn ensure_env_key(existing: Option<&str>, key: &str, value: &str) -> EnvEdit {
    let mut settings = match existing.map(str::trim).filter(|s| !s.is_empty()) {
        None => Map::new(),
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(map)) => map,
            Ok(_) => return EnvEdit::Unusable("settings is not a JSON object".to_string()),
            Err(e) => return EnvEdit::Unusable(format!("settings is not valid JSON: {e}")),
        },
    };

    let env = settings
        .entry("env")
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(env) = env.as_object_mut() else {
        return EnvEdit::Unusable("`env` is not a JSON object".to_string());
    };
    if let Some(current) = env.get(key) {
        return EnvEdit::Present(current.clone());
    }
    env.insert(key.to_string(), Value::String(value.to_string()));

    let mut content =
        serde_json::to_string_pretty(&Value::Object(settings)).unwrap_or_else(|_| "{}".into());
    content.push('\n');
    EnvEdit::Add(content)
}

/// The settings file the command edits. Injected so the command can be tested
/// in memory.
pub trait SettingsFile {
    /// The file content, or `None` when it does not exist.
    fn read(&self) -> Result<Option<String>, String>;
    /// Replaces the file with `content`, returning the backup path when a
    /// previous file was saved aside.
    fn replace(&self, content: &str) -> Result<Option<String>, String>;
}

/// `SettingsFile` over a real path. `replace` copies an existing file to a
/// timestamped backup, then writes a sibling temp file and renames it over the
/// target, so a crash mid-write cannot leave a truncated settings file.
///
/// A symlinked settings file (dotfiles setups) is written through to its
/// target: renaming over the link itself would swap it for a plain file. The
/// previous file's permissions are carried over, since a fresh temp file gets
/// the umask default.
pub struct FsSettingsFile {
    path: PathBuf,
}

impl FsSettingsFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

impl SettingsFile for FsSettingsFile {
    fn read(&self) -> Result<Option<String>, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn replace(&self, content: &str) -> Result<Option<String>, String> {
        let existing = std::fs::metadata(&self.path).ok();
        let target = match existing {
            Some(_) => std::fs::canonicalize(&self.path).map_err(|e| e.to_string())?,
            None => self.path.clone(),
        };
        if let Some(parent) = target.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let backup = if existing.is_some() {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            let backup = sibling(&target, &format!(".bak-{stamp}"));
            std::fs::copy(&target, &backup).map_err(|e| e.to_string())?;
            Some(backup.to_string_lossy().into_owned())
        } else {
            None
        };

        let tmp = sibling(&target, &format!(".atelier-tmp-{}", std::process::id()));
        let written = std::fs::write(&tmp, content)
            .and_then(|()| match &existing {
                Some(meta) => std::fs::set_permissions(&tmp, meta.permissions()),
                None => Ok(()),
            })
            .and_then(|()| std::fs::rename(&tmp, &target));
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }
        Ok(backup)
    }
}
