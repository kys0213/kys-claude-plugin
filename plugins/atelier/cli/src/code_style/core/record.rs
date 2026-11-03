use crate::code_style::core::types::Group;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// What a project has installed: its languages and apps, and every rule
/// (`<group>/<file>`) handed to it — including those it has since deleted,
/// which is how a deleted copy reads as excluded rather than new.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRecord {
    langs: BTreeSet<String>,
    apps: BTreeSet<String>,
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

    pub fn apps(&self) -> &BTreeSet<String> {
        &self.apps
    }

    /// The installed languages and apps (never `Base`), in group order.
    pub fn groups(&self) -> Vec<Group> {
        let langs = self.langs.iter().cloned().map(Group::Lang);
        let apps = self.apps.iter().cloned().map(Group::App);
        langs.chain(apps).collect()
    }

    pub fn has_rule(&self, group: &Group, file: &str) -> bool {
        self.rules.contains(&rule_key(group, file))
    }

    pub fn add(&mut self, group: &Group) {
        match group {
            Group::Base => {}
            Group::Lang(name) => {
                self.langs.insert(name.clone());
            }
            Group::App(name) => {
                self.apps.insert(name.clone());
            }
        }
    }

    /// Drops the group and every rule recorded under it.
    pub fn remove(&mut self, group: &Group) {
        match group {
            Group::Base => {}
            Group::Lang(name) => {
                self.langs.remove(name);
            }
            Group::App(name) => {
                self.apps.remove(name);
            }
        }
        self.set_rules(group, []);
    }

    pub fn add_rule(&mut self, group: &Group, file: &str) {
        self.rules.insert(rule_key(group, file));
    }

    pub fn set_rules(&mut self, group: &Group, files: impl IntoIterator<Item = String>) {
        let prefix = format!("{}/", group.name());
        self.rules.retain(|rule| !rule.starts_with(&prefix));
        for file in files {
            self.rules.insert(rule_key(group, &file));
        }
    }
}

fn rule_key(group: &Group, file: &str) -> String {
    format!("{}/{file}", group.name())
}
