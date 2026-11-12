pub mod check;
pub mod cli;
pub mod install;
pub mod restore;
pub mod uninstall;
pub mod update;

use crate::code_style::core::record::InstallRecord;
use crate::code_style::core::rule_fs::RuleTreeFs;
use crate::code_style::core::types::{
    rule_name, AppliedChange, ApplyAction, ApplyReport, CodeStylePaths, Group, RuleFinding,
    RuleStatus, Selection, RULE_EXT,
};
use crate::drift::core::artifact::BackupClock;
use crate::drift::core::types::ArtifactContent;
use std::collections::BTreeSet;

pub struct CodeStyleDeps<'a> {
    pub fs: &'a dyn RuleTreeFs,
    pub clock: &'a dyn BackupClock,
}

struct JudgedRule {
    finding: RuleFinding,
    copy: String,
    fix: Fix,
}

/// What applying a judged rule does to its copy.
enum Fix {
    Keep,
    Install(String),
    Sync(String),
    Remove,
}

/// The plugin's sources and the project's install as one command sees them.
struct Workspace<'w> {
    deps: &'w CodeStyleDeps<'w>,
    paths: &'w CodeStylePaths,
    langs: BTreeSet<String>,
    apps: BTreeSet<String>,
    installed: BTreeSet<Group>,
    record: Option<InstallRecord>,
}

impl<'w> Workspace<'w> {
    fn load(deps: &'w CodeStyleDeps<'w>, paths: &'w CodeStylePaths) -> Result<Self, String> {
        for dir in [paths.source_root(), paths.source_dir(&Group::Base)] {
            if !deps.fs.is_dir(&dir) {
                return Err(format!("plugin source directory not found: {dir}"));
            }
        }
        let langs = subdirs(deps, &paths.lang_source_root())?;
        let apps = subdirs(deps, &paths.app_source_root())?;
        if let Some(name) = langs.intersection(&apps).next() {
            return Err(format!(
                "{name} is both a language and an app in the plugin sources"
            ));
        }
        let record = load_record(deps, paths)?;
        let installed = match &record {
            Some(record) => record.groups().into_iter().collect(),
            None => subdirs(deps, &paths.install_root())?
                .into_iter()
                .filter(|dir| dir != Group::Base.name())
                .map(Group::Lang)
                .collect(),
        };
        Ok(Workspace {
            deps,
            paths,
            langs,
            apps,
            installed,
            record,
        })
    }

    fn installed_langs(&self) -> BTreeSet<String> {
        self.installed
            .iter()
            .filter_map(|group| match group {
                Group::Lang(name) => Some(name.clone()),
                Group::Base | Group::App(_) => None,
            })
            .collect()
    }

    fn installed_apps(&self) -> BTreeSet<String> {
        self.installed
            .iter()
            .filter_map(|group| match group {
                Group::App(name) => Some(name.clone()),
                Group::Base | Group::Lang(_) => None,
            })
            .collect()
    }

    fn require_installed(&self) -> Result<(), String> {
        if self.installed_langs().is_empty() {
            return Err(format!(
                "no code-style language installed in {} — pass --lang or run install",
                self.paths.install_root()
            ));
        }
        Ok(())
    }

    /// The selected languages and apps, each checked against the plugin.
    fn selected(&self, selection: &Selection) -> Result<BTreeSet<Group>, String> {
        let mut groups = BTreeSet::new();
        for lang in &selection.langs {
            if !self.langs.contains(lang) {
                return Err(format!(
                    "unknown language: {lang} — expected one of: {}",
                    join(&self.langs)
                ));
            }
            groups.insert(Group::Lang(lang.clone()));
        }
        for app in &selection.apps {
            if !self.apps.contains(app) {
                return Err(format!(
                    "unknown app: {app} — expected one of: {}",
                    join(&self.apps)
                ));
            }
            groups.insert(Group::App(app.clone()));
        }
        Ok(groups)
    }

    /// The selection, or every installed language and app when it is empty.
    fn selected_or_installed(&self, selection: &Selection) -> Result<BTreeSet<Group>, String> {
        if selection.is_empty() {
            self.require_installed()?;
            Ok(self.installed.clone())
        } else {
            self.selected(selection)
        }
    }

    fn ships(&self, group: &Group) -> bool {
        match group {
            Group::Base => true,
            Group::Lang(name) => self.langs.contains(name),
            Group::App(name) => self.apps.contains(name),
        }
    }

    fn sources(&self, group: &Group) -> Result<BTreeSet<String>, String> {
        if !self.ships(group) {
            return Ok(BTreeSet::new());
        }
        rule_files(self.deps, &self.paths.source_dir(group))
    }

    fn copies(&self, group: &Group) -> Result<BTreeSet<String>, String> {
        rule_files(self.deps, &self.paths.install_dir(group))
    }

    fn copy_path(&self, group: &Group, file: &str) -> String {
        format!("{}/{file}", self.paths.install_dir(group))
    }

    fn knows(&self, group: &Group, file: &str) -> bool {
        self.record
            .as_ref()
            .is_some_and(|record| record.has_rule(group, file))
    }

    fn source_content(&self, group: &Group, file: &str) -> Result<String, String> {
        read_source(
            self.deps,
            &format!("{}/{file}", self.paths.source_dir(group)),
        )
    }

    fn judge(&self, groups: &BTreeSet<Group>) -> Result<Vec<JudgedRule>, String> {
        let mut judged = Vec::new();
        for group in groups {
            let sources = self.sources(group)?;
            let copies = self.copies(group)?;
            for file in sources.union(&copies) {
                let name = rule_name(group, file);
                let copy = self.copy_path(group, file);
                if !sources.contains(file) {
                    judged.push(JudgedRule {
                        finding: finding(name, RuleStatus::Orphaned, Some(copy.clone())),
                        copy,
                        fix: Fix::Remove,
                    });
                    continue;
                }
                let expected = self.source_content(group, file)?;
                let (finding, fix) = if !copies.contains(file) {
                    if self.knows(group, file) {
                        (finding(name, RuleStatus::Excluded, None), Fix::Keep)
                    } else {
                        (
                            finding(name, RuleStatus::New, Some(copy.clone())),
                            Fix::Install(expected),
                        )
                    }
                } else {
                    match self.deps.fs.read(&copy)? {
                        ArtifactContent::Utf8(current) if current == expected => {
                            (finding(name, RuleStatus::Ok, None), Fix::Keep)
                        }
                        ArtifactContent::Utf8(_) => (
                            finding(name, RuleStatus::Drifted, Some(copy.clone())),
                            Fix::Sync(expected),
                        ),
                        ArtifactContent::NonUtf8 => (
                            finding(
                                name,
                                RuleStatus::Drifted,
                                Some("not valid UTF-8".to_string()),
                            ),
                            Fix::Sync(expected),
                        ),
                    }
                };
                judged.push(JudgedRule { finding, copy, fix });
            }
        }
        Ok(judged)
    }

    /// Every copy of the groups, marked for removal.
    fn removals(&self, groups: &BTreeSet<Group>) -> Result<Vec<JudgedRule>, String> {
        let mut judged = Vec::new();
        for group in groups {
            for file in self.copies(group)? {
                let copy = self.copy_path(group, &file);
                judged.push(JudgedRule {
                    finding: finding(
                        rule_name(group, &file),
                        RuleStatus::Orphaned,
                        Some(copy.clone()),
                    ),
                    copy,
                    fix: Fix::Remove,
                });
            }
        }
        Ok(judged)
    }

    /// The record to update: the stored one, or for an install made before
    /// records existed, one rebuilt from the copies present.
    fn base_record(&self) -> Result<InstallRecord, String> {
        if let Some(record) = &self.record {
            return Ok(record.clone());
        }
        let mut record = InstallRecord::default();
        if self.installed.is_empty() {
            return Ok(record);
        }
        for group in self.installed.iter().chain([&Group::Base]) {
            record.add(group);
            record.set_rules(group, self.copies(group)?);
        }
        Ok(record)
    }

    fn save_record(&self, record: &InstallRecord) -> Result<(), String> {
        if self.record.as_ref() == Some(record) {
            return Ok(());
        }
        self.deps
            .fs
            .write_creating_parents(&self.paths.record_path(), &record.render())
    }

    fn remove_record(&self) -> Result<(), String> {
        let path = self.paths.record_path();
        if self.deps.fs.exists(&path) {
            self.deps.fs.remove_file(&path)?;
        }
        Ok(())
    }
}

/// install and update: bring the groups and base in line, then record what
/// the project now has.
fn sync(ws: &Workspace, groups: BTreeSet<Group>) -> Result<ApplyReport, String> {
    let groups = with_base(groups);
    let judged = ws.judge(&groups)?;

    let mut record = ws.base_record()?;
    for group in &groups {
        if ws.ships(group) {
            record.add(group);
            record.set_rules(group, ws.sources(group)?);
        } else {
            record.remove(group);
        }
    }

    let report = apply(ws.deps, judged)?;
    ws.save_record(&record)?;
    Ok(report)
}

fn with_base(mut groups: BTreeSet<Group>) -> BTreeSet<Group> {
    groups.insert(Group::Base);
    groups
}

fn finding(name: String, status: RuleStatus, detail: Option<String>) -> RuleFinding {
    RuleFinding {
        name,
        status,
        detail,
    }
}

fn join(items: &BTreeSet<String>) -> String {
    if items.is_empty() {
        return "none".to_string();
    }
    items.iter().cloned().collect::<Vec<_>>().join(", ")
}

fn load_record(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
) -> Result<Option<InstallRecord>, String> {
    let path = paths.record_path();
    if !deps.fs.exists(&path) {
        return Ok(None);
    }
    let content = read_copy(deps, &path)?;
    InstallRecord::parse(&content)
        .map(Some)
        .map_err(|e| format!("{path}: {e}"))
}

/// Subdirectory names of `dir`; a missing directory has none.
fn subdirs(deps: &CodeStyleDeps, dir: &str) -> Result<BTreeSet<String>, String> {
    if !deps.fs.is_dir(dir) {
        return Ok(BTreeSet::new());
    }
    Ok(deps.fs.list_dir(dir)?.dirs.into_iter().collect())
}

/// Rule file names in `dir`; a missing directory holds none.
fn rule_files(deps: &CodeStyleDeps, dir: &str) -> Result<BTreeSet<String>, String> {
    if !deps.fs.is_dir(dir) {
        return Ok(BTreeSet::new());
    }
    Ok(deps
        .fs
        .list_dir(dir)?
        .files
        .into_iter()
        .filter(|file| file.ends_with(RULE_EXT))
        .collect())
}

enum Job {
    Install {
        copy: String,
        content: String,
    },
    Sync {
        copy: String,
        current: String,
        content: String,
    },
    Remove {
        copy: String,
        current: String,
    },
}

/// Reads every file it needs before the first write, so a refusal (non-UTF-8
/// copy) leaves the project untouched.
fn apply(deps: &CodeStyleDeps, judged: Vec<JudgedRule>) -> Result<ApplyReport, String> {
    let mut jobs = Vec::new();
    let mut report = ApplyReport::default();
    for JudgedRule { finding, copy, fix } in judged {
        match fix {
            Fix::Keep if finding.status == RuleStatus::Excluded => report.excluded += 1,
            Fix::Keep => report.unchanged += 1,
            Fix::Install(content) => jobs.push(Job::Install { copy, content }),
            Fix::Sync(content) => jobs.push(Job::Sync {
                current: read_copy(deps, &copy)?,
                content,
                copy,
            }),
            Fix::Remove => jobs.push(Job::Remove {
                current: read_copy(deps, &copy)?,
                copy,
            }),
        }
    }

    let stamp = deps.clock.backup_timestamp();
    for job in jobs {
        let change = match job {
            Job::Install { copy, content } => {
                deps.fs.write_creating_parents(&copy, &content)?;
                AppliedChange {
                    path: copy,
                    action: ApplyAction::Installed,
                }
            }
            Job::Sync {
                copy,
                current,
                content,
            } => {
                let backup = backup(deps, &copy, &current, &stamp)?;
                deps.fs.write(&copy, &content)?;
                AppliedChange {
                    path: copy,
                    action: ApplyAction::Synced { backup },
                }
            }
            Job::Remove { copy, current } => {
                let backup = backup(deps, &copy, &current, &stamp)?;
                deps.fs.remove_file(&copy)?;
                AppliedChange {
                    path: copy,
                    action: ApplyAction::Removed { backup },
                }
            }
        };
        report.changes.push(change);
    }
    Ok(report)
}

fn backup(deps: &CodeStyleDeps, copy: &str, current: &str, stamp: &str) -> Result<String, String> {
    let backup_path = format!("{copy}.bak-{stamp}");
    deps.fs.write(&backup_path, current)?;
    Ok(backup_path)
}

fn read_source(deps: &CodeStyleDeps, path: &str) -> Result<String, String> {
    match deps.fs.read(path)? {
        ArtifactContent::Utf8(content) => Ok(content),
        ArtifactContent::NonUtf8 => Err(format!("plugin source file is not valid UTF-8: {path}")),
    }
}

/// Non-UTF-8 bytes cannot go through the `&str` write path, not even into a
/// backup, so such a copy is refused.
fn read_copy(deps: &CodeStyleDeps, path: &str) -> Result<String, String> {
    match deps.fs.read(path)? {
        ArtifactContent::Utf8(content) => Ok(content),
        ArtifactContent::NonUtf8 => Err(format!(
            "{path} is not valid UTF-8 — fix its encoding or remove it, then re-run"
        )),
    }
}
