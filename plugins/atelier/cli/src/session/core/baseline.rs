//! Session baseline storage — the snapshot of the repository taken when a
//! Claude Code session starts, so Stop-time hooks can tell *this session's*
//! changes apart from work that was already sitting in the tree.
//!
//! The trait is what the commands depend on (DIP); `FsBaselineStore` is the
//! only production implementation. Keying by `session_id` is what makes
//! parallel sessions safe: two sessions never touch the same file.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// How long a baseline file survives before a later write prunes it. Sessions
/// never announce their end, so age is the only signal we have.
pub const DEFAULT_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Minimum length of an acceptable session id (see `is_valid_session_id`).
const MIN_SESSION_ID_LEN: usize = 8;

/// Repository state captured at session start.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    /// `HEAD` commit at session start; `None` in an empty repo.
    #[serde(default)]
    pub head: Option<String>,
    /// Paths reported dirty by `git status` at session start — pre-existing
    /// work that must not be attributed to this session.
    #[serde(default)]
    pub dirty: BTreeSet<String>,
    /// Whether this session already got the `/simplify` suggestion. Stop fires
    /// on every turn; the banner is worth showing once.
    #[serde(default)]
    pub notified: bool,
}

impl Baseline {
    /// Records that the session has been notified. A method rather than a raw
    /// field write so the transition stays one place to extend.
    pub fn mark_notified(&mut self) {
        self.notified = true;
    }
}

/// Accepts only `[A-Za-z0-9_-]{8,}`. This is a path-traversal guard first and a
/// sanity check second: the id becomes a file name, so `..`, `/` and `\` must
/// never survive it.
pub fn is_valid_session_id(session_id: &str) -> bool {
    session_id.len() >= MIN_SESSION_ID_LEN
        && session_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub trait BaselineStore {
    /// Returns the stored baseline, or `None` when absent, unreadable, corrupt
    /// or the id is rejected. Reads never fail loudly — this backs an advisory
    /// hook that must stay silent on every edge.
    fn load(&self, session_id: &str) -> Option<Baseline>;

    /// Writes (replacing any existing) baseline for `session_id`.
    fn save(&self, session_id: &str, baseline: &Baseline) -> Result<(), String>;

    /// Writes only when nothing is stored yet, returning whether it wrote.
    /// SessionStart fires again on resume/compact/clear; overwriting there
    /// would erase everything the session has done so far.
    fn save_if_absent(&self, session_id: &str, baseline: &Baseline) -> Result<bool, String> {
        if self.load(session_id).is_some() {
            return Ok(false);
        }
        self.save(session_id, baseline)?;
        Ok(true)
    }
}

/// Baseline store backed by one JSON file per session under a directory.
pub struct FsBaselineStore {
    dir: PathBuf,
    ttl: Duration,
}

/// Disambiguates temp files between concurrent writers in one process; the pid
/// disambiguates between processes.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

impl FsBaselineStore {
    pub fn new(dir: impl Into<PathBuf>, ttl: Duration) -> Self {
        FsBaselineStore {
            dir: dir.into(),
            ttl,
        }
    }

    /// Final path for a session, or `None` when the id is rejected.
    fn path_for(&self, session_id: &str) -> Option<PathBuf> {
        is_valid_session_id(session_id).then(|| self.dir.join(format!("{session_id}.json")))
    }

    /// Deletes baselines (and orphaned temp files) older than the TTL. Called
    /// on write, so the directory is bounded without a background sweeper.
    /// Every failure is ignored: pruning is housekeeping, never a hard error.
    fn prune(&self) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_state_file = matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("json") | Some("tmp")
            );
            if !is_state_file {
                continue;
            }
            if Self::is_expired(&path, self.ttl) {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    /// True when the file's mtime is further in the past than `ttl`. A file
    /// whose mtime is unreadable or in the future is kept.
    fn is_expired(path: &Path, ttl: Duration) -> bool {
        std::fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > ttl)
    }
}

impl BaselineStore for FsBaselineStore {
    fn load(&self, session_id: &str) -> Option<Baseline> {
        let path = self.path_for(session_id)?;
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    fn save(&self, session_id: &str, baseline: &Baseline) -> Result<(), String> {
        let path = self
            .path_for(session_id)
            .ok_or_else(|| format!("invalid session id: {session_id}"))?;
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        // Prune before the write so this session's own file is never a
        // candidate, whatever the TTL is.
        self.prune();

        let json = serde_json::to_string(baseline).map_err(|e| e.to_string())?;
        // Write-then-rename: a concurrent reader sees either the old file or
        // the new one, never a half-written document.
        let tmp = self.dir.join(format!(
            ".{session_id}.{}.{}.tmp",
            std::process::id(),
            TMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })
    }
}
