use crate::code_style::commands::{apply, judge, select_groups, CodeStyleDeps, WhenOmitted};
use crate::code_style::core::types::{ApplyReport, CodeStylePaths};

pub fn run(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    langs: &[String],
) -> Result<ApplyReport, String> {
    let groups = select_groups(deps, paths, langs, WhenOmitted::InstalledLangs)?;
    apply(deps, judge(deps, paths, &groups)?)
}
