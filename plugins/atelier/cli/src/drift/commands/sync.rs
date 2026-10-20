use crate::drift::commands::{read_source, scope_installed, DriftDeps};
use crate::drift::core::types::{
    scan_markers, ArtifactContent, DriftPaths, SyncReport, SyncTarget, POLICY_RULES,
};

pub fn run(
    deps: &DriftDeps,
    paths: &DriftPaths,
    target: SyncTarget,
    name: Option<&str>,
) -> Result<Vec<SyncReport>, String> {
    if let Some(name) = name {
        if !matches!(target, SyncTarget::UserRules | SyncTarget::ProjectRules) {
            return Err(
                "--name is only supported with --target user-rules or project-rules".to_string(),
            );
        }
        if !POLICY_RULES.contains(&name) {
            return Err(format!(
                "unknown policy rule: {name} — expected one of: {}",
                POLICY_RULES.join(", ")
            ));
        }
    }
    match target {
        SyncTarget::ClaudeMd => sync_claude_md(deps, paths).map(|report| vec![report]),
        SyncTarget::Rules => sync_verbatim_copies(
            deps,
            target,
            &[(paths.rules_copy(), paths.template_rules())],
            false,
        ),
        SyncTarget::UserRules | SyncTarget::ProjectRules => {
            let copy = |file: &str| match target {
                SyncTarget::UserRules => paths.user_rule_copy(file),
                _ => paths.project_rule_copy(file),
            };
            let scope: Vec<String> = POLICY_RULES.iter().map(|file| copy(file)).collect();
            let pairs: Vec<(String, String)> = POLICY_RULES
                .iter()
                .filter(|file| name.is_none_or(|n| n == **file))
                .map(|file| (copy(file), paths.template_policy_rule(file)))
                .collect();
            sync_verbatim_copies(deps, target, &pairs, scope_installed(deps, &scope))
        }
    }
}

fn backup(deps: &DriftDeps, target_path: &str, content: &str) -> Result<String, String> {
    let backup_path = format!("{target_path}.bak-{}", deps.clock.backup_timestamp());
    deps.fs.write(&backup_path, content)?;
    Ok(backup_path)
}

/// Non-UTF-8 bytes cannot go through the `&str` write path, not even into a
/// backup, so such a target is refused.
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
    // The line-based rebuild below would convert every CRLF ending in the
    // file to LF — user content outside the markers included.
    if content.contains('\r') {
        return Err(format!(
            "{user_path} uses CRLF line endings — convert to LF or run /atelier:setup to reinstall"
        ));
    }
    let lines: Vec<&str> = content.lines().collect();
    let scan = scan_markers(&lines);
    // Replacing through duplicated or reversed markers would destroy user
    // content outside the block.
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

    // The template carries both markers itself, so it replaces the marker
    // range inclusively.
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
    if new_content.is_empty() {
        new_content.push('\n');
    }
    deps.fs.write(user_path, &new_content)?;

    Ok(SyncReport {
        target: SyncTarget::ClaudeMd,
        path: user_path.clone(),
        backup: Some(backup_path),
    })
}

fn sync_verbatim_copies(
    deps: &DriftDeps,
    target: SyncTarget,
    pairs: &[(String, String)],
    install_missing: bool,
) -> Result<Vec<SyncReport>, String> {
    let mut jobs = Vec::new();
    for (copy_path, template_path) in pairs {
        if !deps.fs.exists(template_path) {
            return Err(format!("plugin source file not found: {template_path}"));
        }
        let current = if deps.fs.exists(copy_path) {
            Some(read_target(deps, copy_path)?)
        } else if install_missing {
            None
        } else {
            return Err(format!("not installed: {copy_path} — run /atelier:setup"));
        };
        let source = read_source(deps, template_path)?;
        jobs.push((copy_path, current, source));
    }

    let stamp = deps.clock.backup_timestamp();
    let mut reports = Vec::new();
    for (copy_path, current, source) in jobs {
        let backup = match current {
            Some(current) => {
                let backup_path = format!("{copy_path}.bak-{stamp}");
                deps.fs.write(&backup_path, &current)?;
                Some(backup_path)
            }
            None => None,
        };
        deps.fs.write(copy_path, &source)?;
        reports.push(SyncReport {
            target,
            path: copy_path.clone(),
            backup,
        });
    }
    Ok(reports)
}
