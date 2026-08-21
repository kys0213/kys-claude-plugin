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
    ArtifactContent, ArtifactStatus, CheckFinding, CheckReport, DriftPaths, BEGIN_MARKER,
    CLAUDE_MD_CHECK, END_MARKER, RULES_CHECK,
};

/// Judges both artifacts against their plugin sources.
pub fn run(deps: &DriftDeps, paths: &DriftPaths) -> Result<CheckReport, String> {
    let template_claude_md = paths.template_claude_md();
    let template_rules = paths.template_rules();
    for template in [&template_claude_md, &template_rules] {
        if !deps.fs.exists(template) {
            return Err(format!("plugin source file not found: {template}"));
        }
    }

    Ok(CheckReport {
        findings: vec![
            check_claude_md(deps, &paths.claude_md, &template_claude_md)?,
            check_rules(deps, &paths.rules_copy(), &template_rules)?,
        ],
    })
}

fn finding(name: &str, status: ArtifactStatus, detail: Option<&str>) -> CheckFinding {
    CheckFinding {
        name: name.to_string(),
        status,
        detail: detail.map(|d| d.to_string()),
    }
}

/// Marker lines are recognized by whole-line equality only, so a prose
/// mention of the marker text never counts.
fn has_line(content: &str, marker: &str) -> bool {
    content.lines().any(|line| line == marker)
}

/// The marker range as a line sequence — same toggle semantics as the shell
/// awk: start collecting at the begin line, stop after the end line, keep
/// scanning (a second begin after the end would reopen collection).
fn extract_block(content: &str) -> Vec<&str> {
    let mut block = Vec::new();
    let mut inside = false;
    for line in content.lines() {
        if line == BEGIN_MARKER {
            inside = true;
        }
        if inside {
            block.push(line);
        }
        if line == END_MARKER {
            inside = false;
        }
    }
    block
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
    match (
        has_line(&content, BEGIN_MARKER),
        has_line(&content, END_MARKER),
    ) {
        // No trace of the block: not installed, not broken.
        (false, false) => Ok(finding(
            CLAUDE_MD_CHECK,
            ArtifactStatus::NotInstalled,
            Some(user_path),
        )),
        (false, true) => Ok(finding(
            CLAUDE_MD_CHECK,
            ArtifactStatus::Drifted,
            Some("begin marker missing"),
        )),
        (true, false) => Ok(finding(
            CLAUDE_MD_CHECK,
            ArtifactStatus::Drifted,
            Some("end marker missing"),
        )),
        (true, true) => {
            // The template file itself contains both markers, so the extracted
            // range compares against the template's full line sequence.
            let template = read_source(deps, template_path)?;
            if extract_block(&content) == template.lines().collect::<Vec<_>>() {
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

fn check_rules(
    deps: &DriftDeps,
    copy_path: &str,
    template_path: &str,
) -> Result<CheckFinding, String> {
    if !deps.fs.exists(copy_path) {
        return Ok(finding(
            RULES_CHECK,
            ArtifactStatus::NotInstalled,
            Some(copy_path),
        ));
    }
    let copy = match deps.fs.read(copy_path)? {
        // Undecodable copy: differs from the UTF-8 source — drift, not error.
        ArtifactContent::NonUtf8 => {
            return Ok(finding(
                RULES_CHECK,
                ArtifactStatus::Drifted,
                Some("not valid UTF-8"),
            ))
        }
        ArtifactContent::Utf8(copy) => copy,
    };
    // setup copies the file verbatim, so identical content is the contract.
    if copy == read_source(deps, template_path)? {
        Ok(finding(RULES_CHECK, ArtifactStatus::Ok, None))
    } else {
        Ok(finding(
            RULES_CHECK,
            ArtifactStatus::Drifted,
            Some(copy_path),
        ))
    }
}
