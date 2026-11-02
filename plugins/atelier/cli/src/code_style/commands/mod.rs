pub mod check;
pub mod install;
pub mod restore;
pub mod uninstall;
pub mod update;

use crate::code_style::core::profile::Profile;
use crate::code_style::core::record::InstallRecord;
use crate::code_style::core::render::render_common;
use crate::code_style::core::rule_fs::RuleTreeFs;
use crate::code_style::core::types::{
    rule_name, AppliedChange, ApplyAction, ApplyReport, CodeStylePaths, RuleFinding, RuleStatus,
    COMMON_GROUP, RULE_EXT,
};
use crate::drift::core::artifact::BackupClock;
use crate::drift::core::types::ArtifactContent;
use std::collections::{BTreeMap, BTreeSet};

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
    available: BTreeSet<String>,
    installed: BTreeSet<String>,
    record: Option<InstallRecord>,
}

impl<'w> Workspace<'w> {
    fn load(deps: &'w CodeStyleDeps<'w>, paths: &'w CodeStylePaths) -> Result<Self, String> {
        for dir in [paths.source_root(), paths.source_dir(COMMON_GROUP)] {
            if !deps.fs.is_dir(&dir) {
                return Err(format!("plugin source directory not found: {dir}"));
            }
        }
        let available = lang_dirs(deps, &paths.source_root())?;
        let record = load_record(deps, paths)?;
        let installed = match &record {
            Some(record) => record.langs().clone(),
            None if deps.fs.is_dir(&paths.install_root()) => {
                lang_dirs(deps, &paths.install_root())?
            }
            None => BTreeSet::new(),
        };
        Ok(Workspace {
            deps,
            paths,
            available,
            installed,
            record,
        })
    }

    fn installed_or_err(&self) -> Result<&BTreeSet<String>, String> {
        if self.installed.is_empty() {
            return Err(format!(
                "no code-style language installed in {} — pass --lang or run install",
                self.paths.install_root()
            ));
        }
        Ok(&self.installed)
    }

    fn available_list(&self) -> String {
        join(&self.available)
    }

    fn requested_langs(&self, requested: &[String]) -> Result<BTreeSet<String>, String> {
        for lang in requested {
            if !self.available.contains(lang) {
                return Err(format!(
                    "unknown language: {lang} — expected one of: {}",
                    self.available_list()
                ));
            }
        }
        Ok(requested.iter().cloned().collect())
    }

    /// The requested languages, or every installed one when none is given.
    fn requested_or_installed(&self, requested: &[String]) -> Result<BTreeSet<String>, String> {
        if requested.is_empty() {
            Ok(self.installed_or_err()?.clone())
        } else {
            self.requested_langs(requested)
        }
    }

    /// Profiles of the languages common rules render for: the installed ones
    /// plus `extra`, limited to languages the plugin still ships.
    fn profiles_with(&self, extra: &BTreeSet<String>) -> Result<BTreeMap<String, Profile>, String> {
        let langs: BTreeSet<String> = self.installed.union(extra).cloned().collect();
        self.profiles_of(&langs)
    }

    fn profiles_of(&self, langs: &BTreeSet<String>) -> Result<BTreeMap<String, Profile>, String> {
        let mut profiles = BTreeMap::new();
        for lang in langs.intersection(&self.available) {
            let path = self.paths.profile_path(lang);
            if !self.deps.fs.exists(&path) {
                return Err(format!("language profile not found: {path}"));
            }
            let profile = Profile::parse(&read_source(self.deps, &path)?)
                .map_err(|e| format!("{path}: {e}"))?;
            profiles.insert(lang.clone(), profile);
        }
        Ok(profiles)
    }

    fn sources(&self, group: &str) -> Result<BTreeSet<String>, String> {
        rule_files(self.deps, &self.paths.source_dir(group))
    }

    fn copies(&self, group: &str) -> Result<BTreeSet<String>, String> {
        rule_files(self.deps, &self.paths.install_dir(group))
    }

    fn copy_path(&self, group: &str, file: &str) -> String {
        format!("{}/{file}", self.paths.install_dir(group))
    }

    fn knows(&self, group: &str, file: &str) -> bool {
        self.record
            .as_ref()
            .is_some_and(|record| record.has_rule(group, file))
    }

    fn expected(
        &self,
        group: &str,
        file: &str,
        profiles: &BTreeMap<String, Profile>,
    ) -> Result<String, String> {
        let path = format!("{}/{file}", self.paths.source_dir(group));
        let source = read_source(self.deps, &path)?;
        if group == COMMON_GROUP {
            render_common(&source, profiles).map_err(|e| format!("{path}: {e}"))
        } else {
            Ok(source)
        }
    }

    fn judge(
        &self,
        groups: &BTreeSet<String>,
        profiles: &BTreeMap<String, Profile>,
    ) -> Result<Vec<JudgedRule>, String> {
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
                let expected = self.expected(group, file, profiles)?;
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

    /// The record to update: the stored one, or for an install made before
    /// records existed, one rebuilt from the copies present.
    fn base_record(&self) -> Result<InstallRecord, String> {
        if let Some(record) = &self.record {
            return Ok(record.clone());
        }
        let mut record = InstallRecord::default();
        for lang in &self.installed {
            record.add_lang(lang);
            record.set_group_rules(lang, self.copies(lang)?);
        }
        if !self.installed.is_empty() {
            record.set_group_rules(COMMON_GROUP, self.copies(COMMON_GROUP)?);
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

/// install and update: bring `langs` and the common rules in line, then
/// record what the project now has.
fn sync(ws: &Workspace, langs: &BTreeSet<String>) -> Result<ApplyReport, String> {
    let profiles = ws.profiles_with(langs)?;
    let groups = with_common(langs);
    let judged = ws.judge(&groups, &profiles)?;

    let mut record = ws.base_record()?;
    for group in &groups {
        if group == COMMON_GROUP || ws.available.contains(group) {
            if group != COMMON_GROUP {
                record.add_lang(group);
            }
            record.set_group_rules(group, ws.sources(group)?);
        } else {
            record.remove_lang(group);
        }
    }

    let report = apply(ws.deps, judged)?;
    ws.save_record(&record)?;
    Ok(report)
}

fn with_common(langs: &BTreeSet<String>) -> BTreeSet<String> {
    let mut groups = langs.clone();
    groups.insert(COMMON_GROUP.to_string());
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

fn lang_dirs(deps: &CodeStyleDeps, root: &str) -> Result<BTreeSet<String>, String> {
    Ok(deps
        .fs
        .list_dir(root)?
        .dirs
        .into_iter()
        .filter(|dir| dir != COMMON_GROUP)
        .collect())
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
