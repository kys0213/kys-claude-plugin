use crate::code_style::commands::{sync, CodeStyleDeps, Workspace};
use crate::code_style::core::types::{ApplyReport, CodeStylePaths, Selection};

pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    selection: &Selection,
) -> Result<ApplyReport, String> {
    let ws = Workspace::load(deps, paths)?;
    let groups = ws.selected_or_installed(selection)?;
    sync(&ws, groups)
}
