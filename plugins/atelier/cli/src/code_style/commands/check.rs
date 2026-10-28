use crate::code_style::commands::{judge, select_groups, CodeStyleDeps, WhenOmitted};
use crate::code_style::core::types::{CheckReport, CodeStylePaths};

pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    langs: &[String],
) -> Result<CheckReport, String> {
    let groups = select_groups(deps, paths, langs, WhenOmitted::InstalledLangs)?;
    let findings = judge(deps, paths, &groups)?
        .into_iter()
        .map(|rule| rule.finding)
        .collect();
    Ok(CheckReport { findings })
}
