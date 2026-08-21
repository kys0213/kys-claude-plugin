//! `drift sync` — deterministically updates one installed copy from its plugin
//! source. It never installs: a missing or corrupted target is refused with an
//! error **before any write**, so every refusal leaves the target byte-for-byte
//! untouched (setup owns installation and repair).
//!
//! On the success path the pre-sync content is backed up to
//! `<file>.bak-<timestamp>` before the overwrite — the one undo lever a
//! deterministic writer can offer.

use crate::drift::commands::{read_source, DriftDeps};
use crate::drift::core::types::{
    ArtifactContent, DriftPaths, SyncReport, SyncTarget, BEGIN_MARKER, END_MARKER,
};

/// Routes the target to its sync routine.
pub fn run(deps: &DriftDeps, paths: &DriftPaths, target: SyncTarget) -> Result<SyncReport, String> {
    match target {
        SyncTarget::ClaudeMd => sync_claude_md(deps, paths),
        SyncTarget::Rules => sync_rules(deps, paths),
    }
}

/// Backs `content` (the target's pre-sync state) up next to the target,
/// returning the backup path.
fn backup(deps: &DriftDeps, target_path: &str, content: &str) -> Result<String, String> {
    let backup_path = format!("{target_path}.bak-{}", deps.clock.backup_timestamp());
    deps.fs.write(&backup_path, content)?;
    Ok(backup_path)
}

/// Reads a sync *target*, refusing what cannot be rewritten byte-faithfully:
/// non-UTF-8 bytes cannot even be backed up through the string-based write
/// path. Refusals happen before any write, so the target stays untouched.
fn read_target(deps: &DriftDeps, path: &str) -> Result<String, String> {
    match deps.fs.read(path)? {
        ArtifactContent::Utf8(content) => Ok(content),
        ArtifactContent::NonUtf8 => Err(format!(
            "{path} is not valid UTF-8 — fix encoding or run /atelier:setup to reinstall"
        )),
    }
}

/// Whole-line marker positions, in order of appearance.
fn marker_positions(lines: &[&str], marker: &str) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, line)| **line == marker)
        .map(|(idx, _)| idx)
        .collect()
}

fn sync_claude_md(deps: &DriftDeps, paths: &DriftPaths) -> Result<SyncReport, String> {
    let template_path = paths.template_claude_md();
    if !deps.fs.exists(&template_path) {
        return Err(format!("plugin source file not found: {template_path}"));
    }
    let user_path = &paths.claude_md;
    if !deps.fs.exists(user_path) {
        return Err(format!(
            "coding-style block not installed in {user_path} — run /atelier:setup"
        ));
    }

    let content = read_target(deps, user_path)?;
    // The line-based rebuild below would silently convert every CRLF ending
    // in the file to LF — user content *outside* the markers included. check
    // stays lenient about CRLF (it only judges content); sync must refuse
    // what it cannot preserve byte-faithfully.
    if content.contains('\r') {
        return Err(format!(
            "{user_path} uses CRLF line endings — convert to LF or run /atelier:setup to reinstall"
        ));
    }
    let lines: Vec<&str> = content.lines().collect();
    let begins = marker_positions(&lines, BEGIN_MARKER);
    let ends = marker_positions(&lines, END_MARKER);
    // The range replacement is only safe against exactly one well-ordered
    // marker pair — forcing it through duplicated or reversed markers would
    // destroy user content outside the block.
    let (begin_idx, end_idx) = match (begins.len(), ends.len()) {
        (0, 0) => {
            return Err(format!(
                "coding-style block not installed in {user_path} — run /atelier:setup"
            ))
        }
        (0, _) | (_, 0) => {
            return Err(format!(
                "broken coding-style block in {user_path} (one marker missing) — run /atelier:setup to reinstall"
            ))
        }
        (1, 1) => (begins[0], ends[0]),
        (begin_count, end_count) => {
            return Err(format!(
                "broken coding-style block in {user_path} (markers duplicated: begin={begin_count}, end={end_count}) — run /atelier:setup to reinstall"
            ))
        }
    };
    if begin_idx >= end_idx {
        return Err(format!(
            "broken coding-style block in {user_path} (markers out of order) — run /atelier:setup to reinstall"
        ));
    }

    let template = read_source(deps, &template_path)?;
    let backup_path = backup(deps, user_path, &content)?;

    // Replace the marker range (inclusive) with the full template line
    // sequence — the template carries both markers itself.
    let mut merged: Vec<&str> = Vec::new();
    merged.extend(&lines[..begin_idx]);
    merged.extend(template.lines());
    merged.extend(&lines[end_idx + 1..]);
    let mut new_content = merged.join("\n");
    new_content.push('\n');
    deps.fs.write(user_path, &new_content)?;

    Ok(SyncReport {
        target: SyncTarget::ClaudeMd,
        path: user_path.clone(),
        backup: backup_path,
    })
}

fn sync_rules(deps: &DriftDeps, paths: &DriftPaths) -> Result<SyncReport, String> {
    let template_path = paths.template_rules();
    if !deps.fs.exists(&template_path) {
        return Err(format!("plugin source file not found: {template_path}"));
    }
    let copy_path = paths.rules_copy();
    if !deps.fs.exists(&copy_path) {
        return Err(format!("not installed: {copy_path} — run /atelier:setup"));
    }

    let current = read_target(deps, &copy_path)?;
    let source = read_source(deps, &template_path)?;
    let backup_path = backup(deps, &copy_path, &current)?;
    deps.fs.write(&copy_path, &source)?;

    Ok(SyncReport {
        target: SyncTarget::Rules,
        path: copy_path,
        backup: backup_path,
    })
}
