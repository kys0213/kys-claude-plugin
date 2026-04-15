use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::fs::FsOps;
use crate::git::GitOps;

/// Maximum number of output entries to keep in history (sliding window).
const MAX_OUTPUT_HISTORY: usize = 10;

/// File extension for loop state files.
pub const STATE_EXT: &str = "state";

/// Build the state file path for a given loop name.
pub fn state_file_path(dir: &Path, loop_name: &str) -> PathBuf {
    dir.join(format!("{loop_name}.{STATE_EXT}"))
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct LoopState {
    pub hash: String,
    pub timestamp: String,
    #[serde(default)]
    pub output_history: Vec<OutputEntry>,
    #[serde(default)]
    pub idle_count: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum OutputCategory {
    #[serde(rename = "gap-analysis")]
    GapAnalysis,
}

impl std::fmt::Display for OutputCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GapAnalysis => write!(f, "gap-analysis"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct OutputEntry {
    pub simhash: String,
    pub category: OutputCategory,
    pub timestamp: String,
}

/// Return the state directory path: /tmp/autopilot-{repo}/state/
pub fn state_dir(git: &dyn GitOps) -> Result<PathBuf> {
    let repo = git.repo_name()?;
    Ok(PathBuf::from(format!("/tmp/autopilot-{repo}/state")))
}

pub fn validate_loop_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains("..") || name.contains('/') || name.contains('\\') {
        anyhow::bail!("invalid loop_name: {name}");
    }
    Ok(())
}

pub fn read_state(fs: &dyn FsOps, state_file: &Path) -> Result<LoopState> {
    let content = fs.read_file(state_file)?;
    let state: LoopState = serde_json::from_str(&content)?;
    Ok(state)
}

pub fn write_state(fs: &dyn FsOps, state_file: &Path, state: &LoopState) -> Result<()> {
    fs.write_file(state_file, &serde_json::to_string(state)?)
}

/// Append an output entry and trim to sliding window.
pub fn append_output_entry(state: &mut LoopState, entry: OutputEntry) {
    state.output_history.push(entry);
    if state.output_history.len() > MAX_OUTPUT_HISTORY {
        let excess = state.output_history.len() - MAX_OUTPUT_HISTORY;
        state.output_history.drain(..excess);
    }
}

/// UTC ISO 8601 timestamp using std::time (no external deps, no process spawn).
pub fn utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_epoch_secs(secs)
}

/// Format unix epoch seconds as ISO 8601 UTC string.
pub fn format_epoch_secs(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let mut days = secs / 86400;

    let mut y = 1970i32;
    loop {
        let year_days = if is_leap(y) { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        y += 1;
    }
    let leap = is_leap(y);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 11usize;
    for (i, &md) in month_days.iter().enumerate() {
        if days < md {
            mo = i;
            break;
        }
        days -= md;
    }

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        mo + 1,
        days + 1,
        h,
        m,
        s
    )
}

fn is_leap(y: i32) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_epoch_secs_unix_epoch() {
        assert_eq!(format_epoch_secs(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_format_epoch_secs_known_date() {
        assert_eq!(format_epoch_secs(1775392245), "2026-04-05T12:30:45Z");
    }

    #[test]
    fn test_format_epoch_secs_leap_year() {
        assert_eq!(format_epoch_secs(1709164800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn test_format_epoch_secs_year_boundary() {
        assert_eq!(format_epoch_secs(1735689600), "2025-01-01T00:00:00Z");
    }

    #[test]
    fn test_format_epoch_secs_dec_31() {
        assert_eq!(format_epoch_secs(31535999), "1970-12-31T23:59:59Z");
    }

    #[test]
    fn test_validate_loop_name_rejects_traversal() {
        assert!(validate_loop_name("../etc/passwd").is_err());
        assert!(validate_loop_name("foo/bar").is_err());
        assert!(validate_loop_name("foo\\bar").is_err());
        assert!(validate_loop_name("").is_err());
    }

    #[test]
    fn test_validate_loop_name_accepts_valid() {
        assert!(validate_loop_name("gap-watch").is_ok());
        assert!(validate_loop_name("test-watch-e2e").is_ok());
        assert!(validate_loop_name("build-issues").is_ok());
    }

    #[test]
    fn test_append_output_entry_sliding_window() {
        let mut state = LoopState {
            hash: "abc".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            output_history: Vec::new(),
            ..Default::default()
        };
        for i in 0..15 {
            append_output_entry(
                &mut state,
                OutputEntry {
                    simhash: format!("0x{i:016X}"),
                    category: OutputCategory::GapAnalysis,
                    timestamp: format!("2026-01-{:02}T00:00:00Z", i + 1),
                },
            );
        }
        assert_eq!(state.output_history.len(), MAX_OUTPUT_HISTORY);
        assert_eq!(state.output_history[0].simhash, "0x0000000000000005");
    }

    #[test]
    fn test_loop_state_backward_compat() {
        let json = r#"{"hash":"aaa1111","timestamp":"2026-01-01T00:00:00Z"}"#;
        let state: LoopState = serde_json::from_str(json).unwrap();
        assert_eq!(state.hash, "aaa1111");
        assert!(state.output_history.is_empty());
    }
}
