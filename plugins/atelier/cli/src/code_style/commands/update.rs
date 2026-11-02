use crate::code_style::commands::{sync, CodeStyleDeps, Workspace};
use crate::code_style::core::types::{ApplyReport, CodeStylePaths};

pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    langs: &[String],
) -> Result<ApplyReport, String> {
    let ws = Workspace::load(deps, paths)?;
    let langs = ws.requested_or_installed(langs)?;
    sync(&ws, &langs)
}
