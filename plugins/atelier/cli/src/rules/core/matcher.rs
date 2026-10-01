use crate::rules::core::rule::Rule;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileMatch {
    pub path: String,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct MatchReport {
    pub files: Vec<FileMatch>,
    pub always_rules: Vec<String>,
    pub unmatched: Vec<String>,
}

/// Every list in the report is sorted, so the same inputs in any order give
/// the same report.
pub fn match_files(rules: &[Rule], files: &[String]) -> MatchReport {
    let mut rules: Vec<&Rule> = rules.iter().collect();
    rules.sort_by(|a, b| a.path().cmp(b.path()));
    let (always, scoped): (Vec<&Rule>, Vec<&Rule>) = rules.into_iter().partition(|r| r.is_always());

    let mut report = MatchReport {
        always_rules: always.iter().map(|r| r.path().to_string()).collect(),
        ..MatchReport::default()
    };
    for file in files.iter().collect::<BTreeSet<_>>() {
        let matched: Vec<String> = scoped
            .iter()
            .filter(|r| r.matches(file))
            .map(|r| r.path().to_string())
            .collect();
        if matched.is_empty() {
            report.unmatched.push(file.clone());
        } else {
            report.files.push(FileMatch {
                path: file.clone(),
                rules: matched,
            });
        }
    }
    report
}
