//! Session inboxes on disk: `<config>/atelier/kanban/inbox/<session>/`, one
//! JSON file per request. A request is written under a hidden temp name and
//! renamed into place, and claimed by renaming it away before it is read, so a
//! reader never sees half a file and no two readers take the same one.

use crate::kanban::core::request::{check_id, Found, Request};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SUFFIX: &str = ".json";

/// `$CLAUDE_CONFIG_DIR`, else `$HOME/.claude`.
pub fn config_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(|home| PathBuf::from(home).join(".claude"))
        .ok_or_else(|| "CLAUDE_CONFIG_DIR 도 HOME 도 없어요".to_string())
}

pub fn inbox_dir(session: &str) -> Result<PathBuf, String> {
    check_id("session", session)?;
    Ok(config_dir()?
        .join("atelier")
        .join("kanban")
        .join("inbox")
        .join(session))
}

fn unique_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:024}-{}", std::process::id())
}

/// Writes the request into the session's inbox and returns its id.
pub fn put(session: &str, request: &Request) -> Result<String, String> {
    let dir = inbox_dir(session)?;
    fs::create_dir_all(&dir).map_err(|e| format!("inbox 를 만들지 못했어요: {e}"))?;
    let id = unique_name();
    let temp = dir.join(format!(".{id}.tmp"));
    fs::write(&temp, request.to_json().to_string())
        .map_err(|e| format!("요청을 쓰지 못했어요: {e}"))?;
    fs::rename(&temp, dir.join(format!("{id}{SUFFIX}")))
        .map_err(|e| format!("요청을 놓지 못했어요: {e}"))?;
    Ok(id)
}

fn age_ms(path: &Path, now: u64) -> u64 {
    let modified = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    now.saturating_sub(modified)
}

/// Claims and removes every finished request, oldest first. A file another
/// reader claimed first is skipped.
pub fn claim_all(session: &str, now: u64) -> Result<Vec<Found>, String> {
    let dir = inbox_dir(session)?;
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|name| !name.starts_with('.'))
        .filter_map(|name| name.strip_suffix(SUFFIX).map(str::to_string))
        .collect();
    ids.sort();
    let mut found = Vec::new();
    for id in ids {
        let path = dir.join(format!("{id}{SUFFIX}"));
        let age_ms = age_ms(&path, now);
        let claimed = dir.join(format!(".{id}.taking"));
        if fs::rename(&path, &claimed).is_err() {
            continue;
        }
        let request = fs::read_to_string(&claimed)
            .ok()
            .and_then(|raw| Request::parse(&raw));
        let _ = fs::remove_file(&claimed);
        found.push(Found {
            id,
            age_ms,
            request,
        });
    }
    Ok(found)
}

pub fn clear(session: &str) -> Result<(), String> {
    let dir = inbox_dir(session)?;
    match fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("inbox 를 지우지 못했어요: {e}")),
    }
}
