use crate::rules::core::matcher::{match_files, MatchReport};
use crate::rules::core::rule::Rule;
use crate::rules::core::source::{ChangedFiles, RuleFiles};

pub struct MatchCommand<'a> {
    changes: &'a dyn ChangedFiles,
    rule_files: &'a dyn RuleFiles,
}

impl<'a> MatchCommand<'a> {
    pub fn new(changes: &'a dyn ChangedFiles, rule_files: &'a dyn RuleFiles) -> Self {
        Self {
            changes,
            rule_files,
        }
    }

    pub fn run(&self, base: &str, head: &str, dirs: &[String]) -> Result<MatchReport, String> {
        let mut rules = Vec::new();
        for dir in dirs {
            for file in self.rule_files.list(dir)? {
                rules.push(Rule::parse(&file.path, &file.content)?);
            }
        }
        let files = self.changes.changed(base, head)?;
        Ok(match_files(&rules, &files))
    }
}
