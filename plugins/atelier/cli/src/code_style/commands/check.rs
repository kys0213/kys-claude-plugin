use crate::code_style::commands::{with_base, CodeStyleDeps, Workspace};
use crate::code_style::core::types::{CheckReport, CodeStylePaths, Selection};

pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    selection: &Selection,
) -> Result<CheckReport, String> {
    let ws = Workspace::load(deps, paths)?;
    let groups = with_base(ws.selected_or_installed(selection)?);
    let findings = ws
        .judge(&groups)?
        .into_iter()
        .map(|rule| rule.finding)
        .collect();
    Ok(CheckReport { findings })
}
