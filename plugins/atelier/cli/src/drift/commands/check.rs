//! `drift check` — judges whether the installed copies (CLAUDE.md block, rules
//! copy) still match their plugin sources. Read-only by construction: the
//! command never calls `ArtifactFs::write`.
//!
//! A missing plugin source is an `Err` (judgement is impossible), while a
//! missing *installed* artifact is a NOT_INSTALLED finding — update never
//! installs, so absence is a report, not a failure. The same line splits
//! encoding problems: an undecodable *user* file is a DRIFTED finding (it
//! cannot equal the UTF-8 source), while an undecodable *source* is an error.
//!
//! check is deliberately more lenient than sync about line endings: `lines()`
//! strips `\r`, so a CRLF-encoded but semantically identical block reports OK
//! — check judges content, while sync refuses to rewrite what it cannot
//! preserve byte-faithfully (see `sync.rs`).

use crate::drift::commands::{read_source, DriftDeps};
use crate::drift::core::types::{
    scan_markers, ArtifactContent, ArtifactStatus, CheckFinding, CheckReport, DriftPaths,
    CLAUDE_MD_CHECK, POLICY_RULES, RULES_CHECK,
};

/// Judges every artifact (CLAUDE.md block, rules copy, policy copies in both
/// scopes) against its plugin source.
pub fn run(deps: &DriftDeps, paths: &DriftPaths) -> Result<CheckReport, String> {
    let template_claude_md = paths.template_claude_md();
    let template_rules = paths.template_rules();
    let policy_templates: Vec<String> = POLICY_RULES
        .iter()
        .map(|name| paths.template_policy_rule(name))
        .collect();
    for template in [&template_claude_md, &template_rules]
        .into_iter()
        .chain(&policy_templates)
    {
        if !deps.fs.exists(template) {
            return Err(format!("plugin source file not found: {template}"));
        }
    }

    let mut findings = vec![
        check_claude_md(deps, &paths.claude_md, &template_claude_md)?,
        check_verbatim_copy(deps, RULES_CHECK, &paths.rules_copy(), &template_rules)?,
    ];
    // Policy copies are judged per scope: the installer chooses user and/or
    // project scope, so an uninstalled scope reports NOT_INSTALLED (not drift).
    for (name, template) in POLICY_RULES.iter().zip(&policy_templates) {
        findings.push(check_verbatim_copy(
            deps,
            &format!("user-rules/{name}"),
            &paths.user_rule_copy(name),
            template,
        )?);
    }
    for (name, template) in POLICY_RULES.iter().zip(&policy_templates) {
        findings.push(check_verbatim_copy(
            deps,
            &format!("project-rules/{name}"),
            &paths.project_rule_copy(name),
            template,
        )?);
    }
    Ok(CheckReport { findings })
}

fn finding(name: &str, status: ArtifactStatus, detail: Option<&str>) -> CheckFinding {
    CheckFinding {
        name: name.to_string(),
        status,
        detail: detail.map(|d| d.to_string()),
    }
}

fn check_claude_md(
    deps: &DriftDeps,
    user_path: &str,
    template_path: &str,
) -> Result<CheckFinding, String> {
    if !deps.fs.exists(user_path) {
        return Ok(finding(
            CLAUDE_MD_CHECK,
            ArtifactStatus::NotInstalled,
            Some(user_path),
        ));
    }
    let content = match deps.fs.read(user_path)? {
        // Undecodable user file: it differs from the UTF-8 source by
        // definition — drift, not an exit-2 error.
        ArtifactContent::NonUtf8 => {
            return Ok(finding(
                CLAUDE_MD_CHECK,
                ArtifactStatus::Drifted,
                Some("not valid UTF-8"),
            ))
        }
        ArtifactContent::Utf8(content) => content,
    };
    let lines: Vec<&str> = content.lines().collect();
    let scan = scan_markers(&lines);
    match (scan.begin, scan.end) {
        // No trace of the block: not installed, not broken.
        (None, None) => Ok(finding(
            CLAUDE_MD_CHECK,
            ArtifactStatus::NotInstalled,
            Some(user_path),
        )),
        (None, Some(_)) => Ok(finding(
            CLAUDE_MD_CHECK,
            ArtifactStatus::Drifted,
            Some("begin marker missing"),
        )),
        (Some(_), None) => Ok(finding(
            CLAUDE_MD_CHECK,
            ArtifactStatus::Drifted,
            Some("end marker missing"),
        )),
        (Some(begin), Some(end)) => {
            // The template file itself contains both markers, so the marker
            // range (inclusive) compares against the template's full line
            // sequence. Duplicated or reversed markers can never equal a
            // template holding exactly one well-ordered pair, so the count
            // and order guards reject them as DRIFTED without a compare.
            let template = read_source(deps, template_path)?;
            let intact = scan.begin_count == 1
                && scan.end_count == 1
                && begin < end
                && lines[begin..=end].iter().copied().eq(template.lines());
            if intact {
                Ok(finding(CLAUDE_MD_CHECK, ArtifactStatus::Ok, None))
            } else {
                Ok(finding(
                    CLAUDE_MD_CHECK,
                    ArtifactStatus::Drifted,
                    Some(user_path),
                ))
            }
        }
    }
}

/// Judges one verbatim-copied artifact (the project rules copy and every
/// user-rules copy share this contract: setup copies the file unchanged, so
/// identical content is the judgement).
fn check_verbatim_copy(
    deps: &DriftDeps,
    name: &str,
    copy_path: &str,
    template_path: &str,
) -> Result<CheckFinding, String> {
    if !deps.fs.exists(copy_path) {
        return Ok(finding(name, ArtifactStatus::NotInstalled, Some(copy_path)));
    }
    let copy = match deps.fs.read(copy_path)? {
        // Undecodable copy: differs from the UTF-8 source — drift, not error.
        ArtifactContent::NonUtf8 => {
            return Ok(finding(
                name,
                ArtifactStatus::Drifted,
                Some("not valid UTF-8"),
            ))
        }
        ArtifactContent::Utf8(copy) => copy,
    };
    if copy == read_source(deps, template_path)? {
        Ok(finding(name, ArtifactStatus::Ok, None))
    } else {
        Ok(finding(name, ArtifactStatus::Drifted, Some(copy_path)))
    }
}
