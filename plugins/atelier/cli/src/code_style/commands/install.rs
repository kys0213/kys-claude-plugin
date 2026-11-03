use crate::code_style::commands::{join, sync, CodeStyleDeps, Workspace};
use crate::code_style::core::types::{ApplyReport, CodeStylePaths, Selection};

pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    selection: &Selection,
) -> Result<ApplyReport, String> {
    let ws = Workspace::load(deps, paths)?;
    if selection.langs.is_empty() {
        return Err(format!(
            "at least one --lang is required (one of: {})",
            join(&ws.langs)
        ));
    }
    let groups = ws.selected(selection)?;
    sync(&ws, groups)
}
