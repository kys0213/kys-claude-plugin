use crate::code_style::commands::{sync, CodeStyleDeps, Workspace};
use crate::code_style::core::types::{ApplyReport, CodeStylePaths};

pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    langs: &[String],
) -> Result<ApplyReport, String> {
    let ws = Workspace::load(deps, paths)?;
    if langs.is_empty() {
        return Err(format!(
            "at least one --lang is required (one of: {})",
            ws.available_list()
        ));
    }
    let langs = ws.requested_langs(langs)?;
    sync(&ws, &langs)
}
