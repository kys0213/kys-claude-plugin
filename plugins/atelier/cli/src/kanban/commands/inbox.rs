//! Session inboxes on disk: `<config>/atelier/kanban/inbox/<session>/`, one
//! JSON file per request. A request is written under a hidden temp name and
//! renamed into place, and claimed by renaming it away before it is read, so a
//! reader never sees half a file and no two readers take the same one.
//! Answers to a permission request wait apart, in `permits/<request id>.json`,
//! for the hook that asked.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SUFFIX: &str = ".json";
const PERMITS: &str = "permits";

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
    fn put_permit(&self, session: &str, request: &str, contents: &str) -> Result<(), String>;
    fn take_permit(
        &self,
        session: &str,
        request: &str,
        now: u64,
    ) -> Result<Option<Claimed>, String>;
}

pub trait Pause {
    fn pause(&self, ms: u64);
}

pub struct ThreadPause;

impl Pause for ThreadPause {
    fn pause(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

pub trait IdSource {
    /// An id that sorts after every id handed out before it.
    fn next_id(&self) -> String;
}

pub struct FsInbox {
    root: Result<PathBuf, String>,
}

pub fn config_dir() -> Result<PathBuf, String> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|h| !h.is_empty())
                .map(|home| PathBuf::from(home).join(".claude"))
        })
        .ok_or_else(|| "CLAUDE_CONFIG_DIR 도 HOME 도 없어요".to_string())
}

impl FsInbox {
    pub fn from_env() -> FsInbox {
        FsInbox {
            root: config_dir().map(|c| c.join("atelier").join("kanban").join("inbox")),
        }
    }
}

/// Writes `contents` under a hidden temp name beside `target` and renames it
/// over `target`, so a reader sees the old file or the whole new one.
pub fn replace_file(target: &Path, contents: &str) -> std::io::Result<()> {
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let temp = target.with_file_name(format!(".{name}.tmp"));
    fs::write(&temp, contents)?;
    fs::rename(&temp, target)
}

fn place(dir: &Path, name: &str, contents: &str) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("inbox 를 만들지 못했어요: {e}"))?;
    replace_file(&dir.join(format!("{name}{SUFFIX}")), contents)
        .map_err(|e| format!("요청을 놓지 못했어요: {e}"))
}

/// `None` when another reader claimed the file first or it is not there.
fn claim(dir: &Path, name: &str, now: u64) -> Option<Claimed> {
    let path = dir.join(format!("{name}{SUFFIX}"));
    let age_ms = age_ms(&path, now);
    let taking = dir.join(format!(".{name}.taking"));
    fs::rename(&path, &taking).ok()?;
    let contents = fs::read_to_string(&taking).ok();
    let _ = fs::remove_file(&taking);
    Some(Claimed {
        id: name.to_string(),
        age_ms,
        contents,
    })
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
        place(&self.dir(session)?, id, contents)
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
        Ok(ids.iter().filter_map(|id| claim(&dir, id, now)).collect())
    }

    fn clear(&self, session: &str) -> Result<(), String> {
        match fs::remove_dir_all(self.dir(session)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("inbox 를 지우지 못했어요: {e}")),
        }
    }

    fn put_permit(&self, session: &str, request: &str, contents: &str) -> Result<(), String> {
        place(&self.dir(session)?.join(PERMITS), request, contents)
    }

    fn take_permit(
        &self,
        session: &str,
        request: &str,
        now: u64,
    ) -> Result<Option<Claimed>, String> {
        Ok(claim(&self.dir(session)?.join(PERMITS), request, now))
    }
}

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
