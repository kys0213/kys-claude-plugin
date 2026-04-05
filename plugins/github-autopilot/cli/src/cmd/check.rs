use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::fs::FsOps;
use crate::git::GitOps;

// Exit codes with business semantics
const EXIT_NO_CHANGES: i32 = 0;
const EXIT_SPEC_CHANGED: i32 = 1;
const EXIT_CODE_CHANGED: i32 = 2;
const EXIT_FIRST_RUN: i32 = 3;

#[derive(Serialize)]
struct DiffResult {
    status: String,
    changed_files: Vec<String>,
    spec_files: Vec<String>,
    code_files: Vec<String>,
}

impl DiffResult {
    fn empty(status: &str) -> Self {
        Self {
            status: status.to_string(),
            changed_files: vec![],
            spec_files: vec![],
            code_files: vec![],
        }
    }
}

fn print_and_exit(result: &DiffResult, exit: i32) -> Result<i32> {
    println!("{}", serde_json::to_string(result)?);
    Ok(exit)
}

#[derive(Serialize, Deserialize)]
struct LoopState {
    hash: String,
    timestamp: String,
}

/// Return the state directory path: /tmp/autopilot-{repo}/state/
fn state_dir(git: &dyn GitOps) -> Result<PathBuf> {
    let repo = git.repo_name()?;
    Ok(PathBuf::from(format!("/tmp/autopilot-{repo}/state")))
}

fn validate_loop_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains("..") || name.contains('/') || name.contains('\\') {
        anyhow::bail!("invalid loop_name: {name}");
    }
    Ok(())
}

/// Check what changed since last analysis.
///
/// Exit codes: 0=no_changes, 1=spec_changed, 2=code_changed, 3=first_run
pub fn diff(
    git: &dyn GitOps,
    fs: &dyn FsOps,
    loop_name: &str,
    spec_paths: &[String],
) -> Result<i32> {
    validate_loop_name(loop_name)?;
    let state_file = state_dir(git)?.join(format!("{loop_name}.state"));

    // Try reading state file; missing file means first run
    let content = match fs.read_file(&state_file) {
        Ok(c) => c,
        Err(_) => return print_and_exit(&DiffResult::empty("first_run"), EXIT_FIRST_RUN),
    };
    let state: LoopState = serde_json::from_str(&content)?;

    if !git.commit_exists(&state.hash)? {
        return print_and_exit(&DiffResult::empty("first_run"), EXIT_FIRST_RUN);
    }

    let current = git.rev_parse_head()?;

    if state.hash == current {
        return print_and_exit(&DiffResult::empty("no_changes"), EXIT_NO_CHANGES);
    }

    let changed = git.diff_name_only(&state.hash, &current)?;

    if changed.is_empty() {
        return print_and_exit(&DiffResult::empty("no_changes"), EXIT_NO_CHANGES);
    }

    let mut spec_files = Vec::new();
    let mut code_files = Vec::new();

    for file in &changed {
        let is_spec = spec_paths
            .iter()
            .any(|prefix| file.starts_with(prefix.trim_end_matches('/')));
        if is_spec {
            spec_files.push(file.clone());
        } else {
            code_files.push(file.clone());
        }
    }

    let (status, exit) = if !spec_files.is_empty() {
        ("spec_changed", EXIT_SPEC_CHANGED)
    } else {
        ("code_changed", EXIT_CODE_CHANGED)
    };

    let result = DiffResult {
        status: status.to_string(),
        changed_files: changed,
        spec_files,
        code_files,
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(exit)
}

/// Record current HEAD as the last analyzed commit.
pub fn mark(git: &dyn GitOps, fs: &dyn FsOps, loop_name: &str) -> Result<i32> {
    validate_loop_name(loop_name)?;
    let state_file = state_dir(git)?.join(format!("{loop_name}.state"));
    let hash = git.rev_parse_head()?;
    let ts = utc_timestamp();

    let state = LoopState {
        hash: hash.clone(),
        timestamp: ts.clone(),
    };
    fs.write_file(&state_file, &serde_json::to_string(&state)?)?;

    println!("marked {loop_name}: {hash} at {ts}");
    Ok(0)
}

/// Show state of all loops.
pub fn status(git: &dyn GitOps, fs: &dyn FsOps) -> Result<i32> {
    let dir = state_dir(git)?;

    let files = match fs.list_files(&dir, "state") {
        Ok(f) => f,
        Err(_) => {
            println!("(no loop states found)");
            return Ok(0);
        }
    };

    if files.is_empty() {
        println!("(no loop states found)");
        return Ok(0);
    }

    println!("{:<20}  {:<9}  TIMESTAMP", "LOOP", "HASH");
    println!("{}", "-".repeat(55));

    for file in &files {
        let loop_name = file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        if let Ok(content) = fs.read_file(file) {
            if let Ok(state) = serde_json::from_str::<LoopState>(&content) {
                let short = &state.hash[..7.min(state.hash.len())];
                println!("{:<20}  {:<9}  {}", loop_name, short, state.timestamp);
            }
        }
    }

    Ok(0)
}

/// UTC ISO 8601 timestamp using std::time (no external deps, no process spawn).
fn utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_epoch_secs(secs)
}

/// Format unix epoch seconds as ISO 8601 UTC string.
pub(crate) fn format_epoch_secs(secs: u64) -> String {
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
        // 2026-04-05T12:30:45Z
        assert_eq!(format_epoch_secs(1775392245), "2026-04-05T12:30:45Z");
    }

    #[test]
    fn test_format_epoch_secs_leap_year() {
        // 2024-02-29T00:00:00Z (leap day)
        assert_eq!(format_epoch_secs(1709164800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn test_format_epoch_secs_year_boundary() {
        // 2025-01-01T00:00:00Z
        assert_eq!(format_epoch_secs(1735689600), "2025-01-01T00:00:00Z");
    }

    #[test]
    fn test_format_epoch_secs_dec_31() {
        // 1970-12-31T23:59:59Z — last second of the year (boundary case)
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
}
