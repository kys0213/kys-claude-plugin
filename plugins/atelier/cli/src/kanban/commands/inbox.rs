//! Session inboxes on disk: `<config>/atelier/kanban/inbox/<session>/`, one
//! JSON file per request. A request is written under a hidden temp name and
//! renamed into place, and claimed by renaming it away before it is read, so a
//! reader never sees half a file and no two readers take the same one.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SUFFIX: &str = ".json";

pub struct Claimed {
    pub id: String,
    pub age_ms: u64,
    /// `None` when the claimed file could not be read.
    pub contents: Option<String>,
}

pub trait InboxStore {
    fn dir(&self, session: &str) -> Result<PathBuf, String>;
    fn put(&self, session: &str, id: &str, contents: &str) -> Result<(), String>;
    /// Claims and removes every finished request, oldest first.
    fn claim_all(&self, session: &str, now: u64) -> Result<Vec<Claimed>, String>;
    fn clear(&self, session: &str) -> Result<(), String>;
}

pub trait IdSource {
    /// An id that sorts after every id handed out before it.
    fn next_id(&self) -> String;
}

pub struct FsInbox {
    root: Result<PathBuf, String>,
}

impl FsInbox {
    /// Under `$CLAUDE_CONFIG_DIR`, else `$HOME/.claude`.
    pub fn from_env() -> FsInbox {
        let config = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|h| !h.is_empty())
                    .map(|home| PathBuf::from(home).join(".claude"))
            })
            .ok_or_else(|| "CLAUDE_CONFIG_DIR 도 HOME 도 없어요".to_string());
        FsInbox {
            root: config.map(|c| c.join("atelier").join("kanban").join("inbox")),
        }
    }
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

impl InboxStore for FsInbox {
    fn dir(&self, session: &str) -> Result<PathBuf, String> {
        Ok(self.root.clone()?.join(session))
    }

    fn put(&self, session: &str, id: &str, contents: &str) -> Result<(), String> {
        let dir = self.dir(session)?;
        fs::create_dir_all(&dir).map_err(|e| format!("inbox 를 만들지 못했어요: {e}"))?;
        let temp = dir.join(format!(".{id}.tmp"));
        fs::write(&temp, contents).map_err(|e| format!("요청을 쓰지 못했어요: {e}"))?;
        fs::rename(&temp, dir.join(format!("{id}{SUFFIX}")))
            .map_err(|e| format!("요청을 놓지 못했어요: {e}"))
    }

    fn claim_all(&self, session: &str, now: u64) -> Result<Vec<Claimed>, String> {
        let dir = self.dir(session)?;
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
        let mut claimed = Vec::new();
        for id in ids {
            let path = dir.join(format!("{id}{SUFFIX}"));
            let age_ms = age_ms(&path, now);
            let taking = dir.join(format!(".{id}.taking"));
            if fs::rename(&path, &taking).is_err() {
                continue;
            }
            let contents = fs::read_to_string(&taking).ok();
            let _ = fs::remove_file(&taking);
            claimed.push(Claimed {
                id,
                age_ms,
                contents,
            });
        }
        Ok(claimed)
    }

    fn clear(&self, session: &str) -> Result<(), String> {
        match fs::remove_dir_all(self.dir(session)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("inbox 를 지우지 못했어요: {e}")),
        }
    }
}

/// Wall-clock nanoseconds and the process id.
pub struct ClockIds;

impl IdSource for ClockIds {
    fn next_id(&self) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{nanos:024}-{}", std::process::id())
    }
}
