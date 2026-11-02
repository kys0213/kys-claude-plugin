use crate::code_style::commands::{
    apply, finding, join, CodeStyleDeps, Fix, JudgedRule, Workspace,
};
use crate::code_style::core::types::{
    rule_name, ApplyReport, CodeStylePaths, RuleStatus, COMMON_GROUP, RULE_EXT,
};
use std::collections::BTreeSet;

/// Puts excluded rules (`<group>/<file>`) back; a rule whose copy is present
/// is left as it is.
pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    rules: &[String],
) -> Result<ApplyReport, String> {
    if rules.is_empty() {
        return Err("at least one --rule is required".to_string());
    }
    let ws = Workspace::load(deps, paths)?;
    let installed = ws.installed_or_err()?;
    let targets = rules
        .iter()
        .map(|rule| target(&ws, installed, rule))
        .collect::<Result<Vec<_>, _>>()?;

    let profiles = ws.profiles_with(&BTreeSet::new())?;
    let mut record = ws.base_record()?;
    let mut judged = Vec::new();
    for (group, file) in targets {
        record.add_rule(&group, &file);
        let copy = ws.copy_path(&group, &file);
        let name = rule_name(&group, &file);
        let rule = if ws.copies(&group)?.contains(&file) {
            JudgedRule {
                finding: finding(name, RuleStatus::Ok, None),
                copy,
                fix: Fix::Keep,
            }
        } else {
            JudgedRule {
                finding: finding(name, RuleStatus::New, Some(copy.clone())),
                fix: Fix::Install(ws.expected(&group, &file, &profiles)?),
                copy,
            }
        };
        judged.push(rule);
    }

    let report = apply(deps, judged)?;
    ws.save_record(&record)?;
    Ok(report)
}

fn target(
    ws: &Workspace,
    installed: &BTreeSet<String>,
    rule: &str,
) -> Result<(String, String), String> {
    let (group, file) = rule
        .split_once('/')
        .filter(|(group, file)| {
            !group.is_empty() && !file.contains('/') && file.ends_with(RULE_EXT)
        })
        .ok_or_else(|| {
            format!("invalid rule: {rule} — expected <group>/<file>.md, e.g. common/comments.md")
        })?;
    if group != COMMON_GROUP && !installed.contains(group) {
        return Err(format!(
            "{rule}: language {group} is not installed (installed: {})",
            join(installed)
        ));
    }
    if !ws.sources(group)?.contains(file) {
        return Err(format!("unknown rule: {rule}"));
    }
    Ok((group.to_string(), file.to_string()))
}
