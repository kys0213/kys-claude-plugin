pub mod check;
pub mod install;
pub mod update;

use crate::code_style::core::rule_fs::RuleTreeFs;
use crate::code_style::core::types::{
    rule_name, AppliedChange, ApplyAction, ApplyReport, CodeStylePaths, RuleFinding, RuleStatus,
    COMMON_GROUP, RULE_EXT,
};
use crate::drift::commands::check::check_verbatim_copy;
use crate::drift::commands::DriftDeps;
use crate::drift::core::artifact::BackupClock;
use crate::drift::core::types::{ArtifactContent, ArtifactStatus};
use std::collections::BTreeSet;

pub struct CodeStyleDeps<'a> {
    pub fs: &'a dyn RuleTreeFs,
    pub clock: &'a dyn BackupClock,
}

impl CodeStyleDeps<'_> {
    fn drift(&self) -> DriftDeps<'_> {
        DriftDeps {
            fs: self.fs,
            clock: self.clock,
        }
    }
}

/// What an empty `--lang` list means for a command.
enum WhenOmitted {
    Refuse,
    InstalledLangs,
}

struct JudgedRule {
    finding: RuleFinding,
    copy: String,
    /// Where the source is or, for an orphaned copy, would be.
    source: String,
}

/// The groups (languages plus `common`) a command works on, sorted by name.
fn select_groups(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    requested: &[String],
    when_omitted: WhenOmitted,
) -> Result<Vec<String>, String> {
    let available = available_langs(deps, paths)?;
    let mut groups: BTreeSet<String> = if requested.is_empty() {
        match when_omitted {
            WhenOmitted::Refuse => {
                return Err(format!(
                    "at least one --lang is required (one of: {})",
                    available.join(", ")
                ))
            }
            WhenOmitted::InstalledLangs => installed_langs(deps, paths)?,
        }
    } else {
        for lang in requested {
            if !available.contains(lang) {
                return Err(format!(
                    "unknown language: {lang} — expected one of: {}",
                    available.join(", ")
                ));
            }
        }
        requested.iter().cloned().collect()
    };
    groups.insert(COMMON_GROUP.to_string());
    Ok(groups.into_iter().collect())
}

fn available_langs(deps: &CodeStyleDeps, paths: &CodeStylePaths) -> Result<Vec<String>, String> {
    for dir in [paths.source_root(), paths.source_dir(COMMON_GROUP)] {
        if !deps.fs.is_dir(&dir) {
            return Err(format!("plugin source directory not found: {dir}"));
        }
    }
    Ok(lang_dirs(deps, &paths.source_root())?.into_iter().collect())
}

fn installed_langs(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
) -> Result<BTreeSet<String>, String> {
    let root = paths.install_root();
    let langs = if deps.fs.is_dir(&root) {
        lang_dirs(deps, &root)?
    } else {
        BTreeSet::new()
    };
    if langs.is_empty() {
        return Err(format!(
            "no code-style language installed in {root} — pass --lang or run install"
        ));
    }
    Ok(langs)
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

fn judge(
    deps: &CodeStyleDeps,
    paths: &CodeStylePaths,
    groups: &[String],
) -> Result<Vec<JudgedRule>, String> {
    let mut judged = Vec::new();
    for group in groups {
        let source_dir = paths.source_dir(group);
        let install_dir = paths.install_dir(group);
        let sources = rule_files(deps, &source_dir)?;
        let copies = rule_files(deps, &install_dir)?;
        for file in sources.union(&copies) {
            let name = rule_name(group, file);
            let copy = format!("{install_dir}/{file}");
            let source = format!("{source_dir}/{file}");
            let finding = if sources.contains(file) {
                let finding =
                    check_verbatim_copy(&deps.drift(), &name, &copy, &source, ArtifactStatus::New)?;
                RuleFinding {
                    name: finding.name,
                    status: finding.status.into(),
                    detail: finding.detail,
                }
            } else {
                RuleFinding {
                    name,
                    status: RuleStatus::Orphaned,
                    detail: Some(copy.clone()),
                }
            };
            judged.push(JudgedRule {
                finding,
                copy,
                source,
            });
        }
    }
    Ok(judged)
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
/// copy or source) leaves the project untouched.
fn apply(deps: &CodeStyleDeps, judged: Vec<JudgedRule>) -> Result<ApplyReport, String> {
    let mut jobs = Vec::new();
    let mut unchanged = 0;
    for rule in judged {
        let JudgedRule {
            finding,
            copy,
            source,
        } = rule;
        match finding.status {
            RuleStatus::Ok => unchanged += 1,
            RuleStatus::New => jobs.push(Job::Install {
                content: read_source(deps, &source)?,
                copy,
            }),
            RuleStatus::Drifted => jobs.push(Job::Sync {
                current: read_copy(deps, &copy)?,
                content: read_source(deps, &source)?,
                copy,
            }),
            RuleStatus::Orphaned => jobs.push(Job::Remove {
                current: read_copy(deps, &copy)?,
                copy,
            }),
        }
    }

    let stamp = deps.clock.backup_timestamp();
    let mut changes = Vec::new();
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
        changes.push(change);
    }
    Ok(ApplyReport { changes, unchanged })
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
