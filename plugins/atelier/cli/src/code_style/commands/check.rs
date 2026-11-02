use crate::code_style::commands::{with_common, CodeStyleDeps, Workspace};
use crate::code_style::core::types::{CheckReport, CodeStylePaths};

pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    langs: &[String],
) -> Result<CheckReport, String> {
    let ws = Workspace::load(deps, paths)?;
    let langs = ws.requested_or_installed(langs)?;
    let profiles = ws.profiles_with(&langs)?;
    let findings = ws
        .judge(&with_common(&langs), &profiles)?
        .into_iter()
        .map(|rule| rule.finding)
        .collect();
    Ok(CheckReport { findings })
}
