use crate::drift::commands::{read_source, scope_installed, DriftDeps};
use crate::drift::core::types::{
    scan_markers, ArtifactContent, ArtifactStatus, CheckFinding, CheckReport, DriftPaths,
    CLAUDE_MD_CHECK, POLICY_RULES,
};

pub fn run(deps: &DriftDeps, paths: &DriftPaths) -> Result<CheckReport, String> {
    let template_claude_md = paths.template_claude_md();
    let scoped_templates: Vec<String> = POLICY_RULES
        .iter()
        .map(|name| paths.template_policy_rule(name))
        .collect();
    for template in std::iter::once(&template_claude_md).chain(&scoped_templates) {
        if !deps.fs.exists(template) {
            return Err(format!("plugin source file not found: {template}"));
        }
    }

    let mut findings = vec![check_claude_md(
        deps,
        &paths.claude_md,
        &template_claude_md,
    )?];
    findings.extend(check_scope(
        deps,
        "user-rules",
        POLICY_RULES,
        |name| paths.user_rule_copy(name),
        |name| paths.template_policy_rule(name),
    )?);
    Ok(CheckReport { findings })
}

/// A set of copies installed together: once any is there, a missing one is a
/// file a newer plugin version added.
fn check_scope(
    deps: &DriftDeps,
    label: &str,
    names: &[&str],
    copy_of: impl Fn(&str) -> String,
    template_of: impl Fn(&str) -> String,
) -> Result<Vec<CheckFinding>, String> {
    let copies: Vec<String> = names.iter().map(|name| copy_of(name)).collect();
    let absent = if scope_installed(deps, &copies) {
        ArtifactStatus::New
    } else {
        ArtifactStatus::NotInstalled
    };
    names
        .iter()
        .zip(&copies)
        .map(|(name, copy)| {
            check_verbatim_copy(
                deps,
                &format!("{label}/{name}"),
                copy,
                &template_of(name),
                absent,
            )
        })
        .collect()
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
            // The template carries both markers, so the inclusive marker range
            // compares against the whole template. `lines()` strips `\r`, so a
            // CRLF block with the template's content is OK here even though
            // sync refuses to rewrite it.
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

pub(crate) fn check_verbatim_copy(
    deps: &DriftDeps,
    name: &str,
    copy_path: &str,
    template_path: &str,
    absent: ArtifactStatus,
) -> Result<CheckFinding, String> {
    if !deps.fs.exists(copy_path) {
        return Ok(finding(name, absent, Some(copy_path)));
    }
    let copy = match deps.fs.read(copy_path)? {
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
