use crate::code_style::commands::{apply, finding, CodeStyleDeps, Fix, JudgedRule, Workspace};
use crate::code_style::core::types::{
    rule_name, ApplyReport, CodeStylePaths, Group, RuleStatus, RULE_EXT,
};

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
    ws.require_installed()?;
    let targets = rules
        .iter()
        .map(|rule| target(&ws, rule))
        .collect::<Result<Vec<_>, _>>()?;

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
                fix: Fix::Install(ws.source_content(&group, &file)?),
                copy,
            }
        };
        judged.push(rule);
    }

    let report = apply(deps, judged)?;
    ws.save_record(&record)?;
    Ok(report)
}

fn target(ws: &Workspace, rule: &str) -> Result<(Group, String), String> {
    let (name, file) = rule
        .split_once('/')
        .filter(|(name, file)| !name.is_empty() && !file.contains('/') && file.ends_with(RULE_EXT))
        .ok_or_else(|| {
            format!("invalid rule: {rule} — expected <group>/<file>.md, e.g. base/comments.md")
        })?;
    let group = if name == Group::Base.name() {
        Group::Base
    } else {
        ws.installed
            .iter()
            .find(|group| group.name() == name)
            .cloned()
            .ok_or_else(|| {
                let installed: Vec<&str> = ws.installed.iter().map(Group::name).collect();
                format!(
                    "{rule}: {name} is not installed (installed: {})",
                    installed.join(", ")
                )
            })?
    };
    if !ws.sources(&group)?.contains(file) {
        return Err(format!("unknown rule: {rule}"));
    }
    Ok((group, file.to_string()))
}
