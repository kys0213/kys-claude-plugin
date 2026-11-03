use crate::code_style::commands::{apply, join, CodeStyleDeps, Workspace};
use crate::code_style::core::types::{ApplyReport, CodeStylePaths, Group, Selection};
use std::collections::BTreeSet;

/// Backs up and removes the selected languages' and apps' copies. Removing
/// the last language removes base, every app and the install record too.
pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    selection: &Selection,
) -> Result<ApplyReport, String> {
    if selection.is_empty() {
        return Err("at least one --lang or --app is required".to_string());
    }
    let ws = Workspace::load(deps, paths)?;
    let installed_langs = ws.installed_langs();
    let installed_apps = ws.installed_apps();
    for lang in &selection.langs {
        if !installed_langs.contains(lang) {
            return Err(format!(
                "language not installed: {lang} (installed: {})",
                join(&installed_langs)
            ));
        }
    }
    for app in &selection.apps {
        if !installed_apps.contains(app) {
            return Err(format!(
                "app not installed: {app} (installed: {})",
                join(&installed_apps)
            ));
        }
    }

    let leaves_no_language = installed_langs
        .iter()
        .all(|lang| selection.langs.contains(lang));
    let removed: BTreeSet<Group> = if leaves_no_language {
        ws.installed.iter().cloned().chain([Group::Base]).collect()
    } else {
        let langs = selection.langs.iter().cloned().map(Group::Lang);
        let apps = selection.apps.iter().cloned().map(Group::App);
        langs.chain(apps).collect()
    };

    let mut record = ws.base_record()?;
    for group in &removed {
        record.remove(group);
    }
    let report = apply(deps, ws.removals(&removed)?)?;
    if leaves_no_language {
        ws.remove_record()?;
    } else {
        ws.save_record(&record)?;
    }
    Ok(report)
}
