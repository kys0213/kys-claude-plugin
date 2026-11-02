use crate::code_style::commands::{
    apply, finding, join, CodeStyleDeps, Fix, JudgedRule, Workspace,
};
use crate::code_style::core::types::{
    rule_name, ApplyReport, CodeStylePaths, RuleStatus, COMMON_GROUP,
};
use std::collections::BTreeSet;

/// Backs up and removes the languages' copies and re-renders the common rules
/// for the languages left. Removing the last language removes the common
/// rules and the install record too.
pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    langs: &[String],
) -> Result<ApplyReport, String> {
    let ws = Workspace::load(deps, paths)?;
    let installed = ws.installed_or_err()?;
    if langs.is_empty() {
        return Err(format!(
            "at least one --lang is required (installed: {})",
            join(installed)
        ));
    }
    for lang in langs {
        if !installed.contains(lang) {
            return Err(format!(
                "language not installed: {lang} (installed: {})",
                join(installed)
            ));
        }
    }
    let removed: BTreeSet<String> = langs.iter().cloned().collect();
    let remaining: BTreeSet<String> = installed.difference(&removed).cloned().collect();

    let mut judged = Vec::new();
    for lang in &removed {
        judged.extend(removals(&ws, lang)?);
    }
    let mut record = ws.base_record()?;
    for lang in &removed {
        record.remove_lang(lang);
    }
    if remaining.is_empty() {
        judged.extend(removals(&ws, COMMON_GROUP)?);
    } else {
        let profiles = ws.profiles_of(&remaining)?;
        let common = BTreeSet::from([COMMON_GROUP.to_string()]);
        judged.extend(ws.judge(&common, &profiles)?);
        record.set_group_rules(COMMON_GROUP, ws.sources(COMMON_GROUP)?);
    }

    let report = apply(deps, judged)?;
    if remaining.is_empty() {
        ws.remove_record()?;
    } else {
        ws.save_record(&record)?;
    }
    Ok(report)
}

fn removals(ws: &Workspace, group: &str) -> Result<Vec<JudgedRule>, String> {
    Ok(ws
        .copies(group)?
        .into_iter()
        .map(|file| {
            let copy = ws.copy_path(group, &file);
            JudgedRule {
                finding: finding(
                    rule_name(group, &file),
                    RuleStatus::Orphaned,
                    Some(copy.clone()),
                ),
                copy,
                fix: Fix::Remove,
            }
        })
        .collect())
}
