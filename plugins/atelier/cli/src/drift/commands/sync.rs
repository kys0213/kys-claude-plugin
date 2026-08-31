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
    scan_markers, ArtifactContent, DriftPaths, SyncReport, SyncTarget, USER_RULES,
};

/// Routes the target to its sync routine. A target is one `--target` value,
/// not one file: user-rules covers every `USER_RULES` copy, so the result is a
/// report list (single-element for the other targets).
pub fn run(
    deps: &DriftDeps,
    paths: &DriftPaths,
    target: SyncTarget,
) -> Result<Vec<SyncReport>, String> {
    match target {
        SyncTarget::ClaudeMd => sync_claude_md(deps, paths).map(|report| vec![report]),
        SyncTarget::Rules => sync_verbatim_copies(
            deps,
            target,
            &[(paths.rules_copy(), paths.template_rules())],
        ),
        SyncTarget::UserRules => {
            let pairs: Vec<(String, String)> = USER_RULES
                .iter()
                .map(|name| (paths.user_rule_copy(name), paths.template_user_rule(name)))
                .collect();
            sync_verbatim_copies(deps, target, &pairs)
        }
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
    let scan = scan_markers(&lines);
    // The range replacement is only safe against exactly one well-ordered
    // marker pair — forcing it through duplicated or reversed markers would
    // destroy user content outside the block.
    let (begin_idx, end_idx) = match (scan.begin, scan.end) {
        (None, None) => {
            return Err(format!(
                "coding-style block not installed in {user_path} — run /atelier:setup"
            ))
        }
        (None, Some(_)) | (Some(_), None) => {
            return Err(format!(
                "broken coding-style block in {user_path} (one marker missing) — run /atelier:setup to reinstall"
            ))
        }
        (Some(begin), Some(end)) => {
            if scan.begin_count != 1 || scan.end_count != 1 {
                return Err(format!(
                    "broken coding-style block in {user_path} (markers duplicated: begin={}, end={}) — run /atelier:setup to reinstall",
                    scan.begin_count, scan.end_count
                ));
            }
            (begin, end)
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
    // sequence — the template carries both markers itself. Every line gets a
    // trailing '\n' (join-plus-final-newline semantics).
    let mut new_content = String::with_capacity(content.len() + template.len());
    let merged = lines[..begin_idx]
        .iter()
        .copied()
        .chain(template.lines())
        .chain(lines[end_idx + 1..].iter().copied());
    for line in merged {
        new_content.push_str(line);
        new_content.push('\n');
    }
    // Faithful to the previous `join("\n")` + `push('\n')`: an all-empty
    // merge (empty template, block spanning the whole file) still writes a
    // single newline.
    if new_content.is_empty() {
        new_content.push('\n');
    }
    deps.fs.write(user_path, &new_content)?;

    Ok(SyncReport {
        target: SyncTarget::ClaudeMd,
        path: user_path.clone(),
        backup: backup_path,
    })
}

/// Syncs verbatim-copy artifacts — the write-side mirror of
/// `check_verbatim_copy`: the project rules copy and every user-rules copy
/// share one contract, `(copy, template)` pairs overwritten wholesale.
///
/// The pair list syncs as a unit: every source and every installed copy is
/// validated (and read) before the first write, so a refusal anywhere leaves
/// all copies byte-for-byte untouched. One timestamp stamps every backup of
/// the run.
fn sync_verbatim_copies(
    deps: &DriftDeps,
    target: SyncTarget,
    pairs: &[(String, String)],
) -> Result<Vec<SyncReport>, String> {
    let mut jobs = Vec::new();
    for (copy_path, template_path) in pairs {
        if !deps.fs.exists(template_path) {
            return Err(format!("plugin source file not found: {template_path}"));
        }
        if !deps.fs.exists(copy_path) {
            return Err(format!("not installed: {copy_path} — run /atelier:setup"));
        }
        let current = read_target(deps, copy_path)?;
        let source = read_source(deps, template_path)?;
        jobs.push((copy_path, current, source));
    }

    let stamp = deps.clock.backup_timestamp();
    let mut reports = Vec::new();
    for (copy_path, current, source) in jobs {
        let backup_path = format!("{copy_path}.bak-{stamp}");
        deps.fs.write(&backup_path, &current)?;
        deps.fs.write(copy_path, &source)?;
        reports.push(SyncReport {
            target,
            path: copy_path.clone(),
            backup: backup_path,
        });
    }
    Ok(reports)
}
