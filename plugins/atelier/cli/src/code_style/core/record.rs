use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// What a project has installed: its languages, and every rule
/// (`<group>/<file>`) handed to it — including those it has since deleted,
/// which is how a deleted copy reads as excluded rather than new.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRecord {
    langs: BTreeSet<String>,
    rules: BTreeSet<String>,
}

impl InstallRecord {
    pub fn parse(json: &str) -> Result<InstallRecord, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn render(&self) -> String {
        let mut out = serde_json::to_string_pretty(self).expect("sets of strings serialize");
        out.push('\n');
        out
    }

    pub fn langs(&self) -> &BTreeSet<String> {
        &self.langs
    }

    pub fn has_rule(&self, group: &str, file: &str) -> bool {
        self.rules.contains(&rule_key(group, file))
    }

    pub fn add_lang(&mut self, lang: &str) {
        self.langs.insert(lang.to_string());
    }

    pub fn add_rule(&mut self, group: &str, file: &str) {
        self.rules.insert(rule_key(group, file));
    }

    /// Drops the language and every rule recorded under it.
    pub fn remove_lang(&mut self, lang: &str) {
        self.langs.remove(lang);
        self.set_group_rules(lang, []);
    }

    pub fn set_group_rules(&mut self, group: &str, files: impl IntoIterator<Item = String>) {
        let prefix = format!("{group}/");
        self.rules.retain(|rule| !rule.starts_with(&prefix));
        for file in files {
            self.rules.insert(rule_key(group, &file));
        }
    }
}

fn rule_key(group: &str, file: &str) -> String {
    format!("{group}/{file}")
}
