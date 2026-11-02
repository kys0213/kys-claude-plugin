//! Shared test doubles and fixtures for the code-style subsystem tests. The
//! filesystem is in-memory and its `write` refuses a path whose parent
//! directory does not exist, like the real one — so a fresh install that
//! forgets to create its directories fails here too.
//!
//! `#![allow(dead_code)]` because each test binary uses only a subset (Cargo
//! compiles this module separately into every test crate).
#![allow(dead_code)]

use atelier::code_style::commands::CodeStyleDeps;
use atelier::code_style::core::rule_fs::{DirEntries, RuleTreeFs};
use atelier::code_style::core::types::CodeStylePaths;
use atelier::drift::core::artifact::{ArtifactFs, BackupClock};
use atelier::drift::core::types::ArtifactContent;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

pub const TS: &str = "20260821-120000";

pub const PLUGIN_ROOT: &str = "/plugin";
pub const PROJECT_DIR: &str = "/proj";

pub const GO_RULES: &[&str] = &["go-errors.md", "go-naming.md"];
pub const RUST_RULES: &[&str] = &["rust-naming.md"];
pub const COMMON_RULES: &[&str] = &["cohesion.md", "comments.md"];

pub fn rules_of(group: &str) -> &'static [&'static str] {
    match group {
        "go" => GO_RULES,
        "rust" => RUST_RULES,
        "common" => COMMON_RULES,
        other => panic!("no fixture group {other}"),
    }
}

pub fn source(group: &str, file: &str) -> String {
    format!("{PLUGIN_ROOT}/rules/code-style/{group}/{file}")
}

pub fn profile_path(lang: &str) -> String {
    source(lang, "profile.json")
}

pub fn copy(group: &str, file: &str) -> String {
    format!("{PROJECT_DIR}/.claude/rules/atelier/code-style/{group}/{file}")
}

pub fn record_path() -> String {
    format!("{PROJECT_DIR}/.claude/rules/atelier/code-style/installed.json")
}

/// The source glob every fixture common rule (`roles: [source]`) renders to.
pub fn source_glob(lang: &str) -> &'static str {
    match lang {
        "go" => "**/*.go",
        "rust" => "**/*.rs",
        other => panic!("no fixture profile {other}"),
    }
}

pub fn profile_json(lang: &str) -> String {
    let ext = source_glob(lang).trim_start_matches("**/*");
    format!(
        "{{\"roles\": {{\"source\": [\"{}\"], \"config\": [\"**/config{ext}\"], \
         \"boundary\": [\"**/*_client{ext}\"], \"test\": [\"**/*_test{ext}\"]}}}}\n",
        source_glob(lang)
    )
}

/// A source file's content: a language rule is plain, a common rule declares
/// `roles: [source]`.
pub fn body(group: &str, file: &str) -> String {
    if group == "common" {
        common_source(file, "roles: [source]")
    } else {
        format!("# {group}/{file}\n\n- DO keep it simple\n")
    }
}

pub fn common_source(file: &str, roles_line: &str) -> String {
    format!("---\nname: {file}\n{roles_line}\n---\n\n# common/{file}\n")
}

/// What a fixture common rule installs as for `langs`.
pub fn rendered(file: &str, langs: &[&str]) -> String {
    let mut sorted = langs.to_vec();
    sorted.sort();
    sorted.dedup();
    let globs: String = sorted
        .iter()
        .map(|lang| format!("  - \"{}\"\n", source_glob(lang)))
        .collect();
    format!("---\nname: {file}\npaths:\n{globs}---\n\n# common/{file}\n")
}

/// What `group/file` installs as when `langs` are installed.
pub fn expected(group: &str, file: &str, langs: &[&str]) -> String {
    if group == "common" {
        rendered(file, langs)
    } else {
        body(group, file)
    }
}

pub fn record_json(langs: &[&str], rules: &[&str]) -> String {
    let list = |items: Vec<String>| -> String {
        if items.is_empty() {
            "[]".to_string()
        } else {
            let lines: Vec<String> = items.iter().map(|i| format!("    \"{i}\"")).collect();
            format!("[\n{}\n  ]", lines.join(",\n"))
        }
    };
    let mut langs: Vec<String> = langs.iter().map(|l| l.to_string()).collect();
    langs.sort();
    langs.dedup();
    let mut rules: Vec<String> = rules.iter().map(|r| r.to_string()).collect();
    rules.sort();
    rules.dedup();
    format!(
        "{{\n  \"langs\": {},\n  \"rules\": {}\n}}\n",
        list(langs),
        list(rules)
    )
}

/// Every `group/file` name a full install of `langs` records.
pub fn all_rules(langs: &[&str]) -> Vec<String> {
    let mut names = Vec::new();
    for group in langs.iter().copied().chain(["common"]) {
        for file in rules_of(group) {
            names.push(format!("{group}/{file}"));
        }
    }
    names
}

pub fn full_record(langs: &[&str]) -> String {
    let rules = all_rules(langs);
    let refs: Vec<&str> = rules.iter().map(String::as_str).collect();
    record_json(langs, &refs)
}

pub fn paths() -> CodeStylePaths {
    CodeStylePaths {
        plugin_root: PLUGIN_ROOT.to_string(),
        project_dir: PROJECT_DIR.to_string(),
    }
}

pub fn langs(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[derive(Default)]
pub struct MemFs {
    pub files: RefCell<BTreeMap<String, Vec<u8>>>,
    pub empty_dirs: RefCell<BTreeSet<String>>,
    pub writes: RefCell<Vec<(String, String)>>,
    pub removals: RefCell<Vec<String>>,
}

impl MemFs {
    /// Every fixture source (go, rust, common) with profiles, nothing installed.
    pub fn with_sources() -> Self {
        let fs = MemFs::default();
        for group in ["go", "rust", "common"] {
            for file in rules_of(group) {
                fs.insert(&source(group, file), &body(group, file));
            }
        }
        for lang in ["go", "rust"] {
            fs.insert(&profile_path(lang), &profile_json(lang));
        }
        fs
    }

    /// Sources plus a full, in-sync install of `langs` with its record.
    pub fn installed(langs: &[&str]) -> Self {
        let fs = MemFs::installed_without_record(langs);
        fs.insert(&record_path(), &full_record(langs));
        fs
    }

    /// Sources plus an in-sync install of `langs` from before install records.
    pub fn installed_without_record(langs: &[&str]) -> Self {
        let fs = MemFs::with_sources();
        for lang in langs {
            for file in rules_of(lang) {
                fs.insert(&copy(lang, file), &body(lang, file));
            }
        }
        for file in COMMON_RULES {
            fs.insert(&copy("common", file), &rendered(file, langs));
        }
        fs
    }

    pub fn insert(&self, path: &str, content: &str) {
        self.insert_bytes(path, content.as_bytes());
    }

    pub fn insert_bytes(&self, path: &str, bytes: &[u8]) {
        self.files
            .borrow_mut()
            .insert(path.to_string(), bytes.to_vec());
    }

    pub fn delete(&self, path: &str) {
        self.files.borrow_mut().remove(path);
    }

    pub fn mkdir(&self, path: &str) {
        self.empty_dirs.borrow_mut().insert(path.to_string());
    }

    pub fn content(&self, path: &str) -> Option<String> {
        self.files
            .borrow()
            .get(path)
            .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
    }

    pub fn mutation_count(&self) -> usize {
        self.writes.borrow().len() + self.removals.borrow().len()
    }
}

impl ArtifactFs for MemFs {
    fn exists(&self, path: &str) -> bool {
        self.files.borrow().contains_key(path)
    }

    fn read(&self, path: &str) -> Result<ArtifactContent, String> {
        let bytes = self
            .files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| format!("{path}: not found"))?;
        Ok(match String::from_utf8(bytes) {
            Ok(content) => ArtifactContent::Utf8(content),
            Err(_) => ArtifactContent::NonUtf8,
        })
    }

    fn write(&self, path: &str, content: &str) -> Result<(), String> {
        let parent = path.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        if !self.is_dir(parent) {
            return Err(format!("{path}: No such file or directory"));
        }
        self.writes
            .borrow_mut()
            .push((path.to_string(), content.to_string()));
        self.insert(path, content);
        Ok(())
    }
}

impl RuleTreeFs for MemFs {
    fn is_dir(&self, path: &str) -> bool {
        let prefix = format!("{path}/");
        self.files.borrow().keys().any(|k| k.starts_with(&prefix))
            || self
                .empty_dirs
                .borrow()
                .iter()
                .any(|d| d == path || d.starts_with(&prefix))
    }

    fn list_dir(&self, path: &str) -> Result<DirEntries, String> {
        if !self.is_dir(path) {
            return Err(format!("{path}: No such file or directory"));
        }
        let prefix = format!("{path}/");
        let mut files = BTreeSet::new();
        let mut dirs = BTreeSet::new();
        let empty_dirs = self.empty_dirs.borrow();
        let keys = self.files.borrow().keys().cloned().collect::<Vec<_>>();
        for key in keys.iter().chain(empty_dirs.iter()) {
            let Some(rest) = key.strip_prefix(&prefix) else {
                continue;
            };
            match rest.split_once('/') {
                Some((dir, _)) => {
                    dirs.insert(dir.to_string());
                }
                None if empty_dirs.contains(key) => {
                    dirs.insert(rest.to_string());
                }
                None => {
                    files.insert(rest.to_string());
                }
            }
        }
        Ok(DirEntries {
            files: files.into_iter().collect(),
            dirs: dirs.into_iter().collect(),
        })
    }

    fn write_creating_parents(&self, path: &str, content: &str) -> Result<(), String> {
        if let Some((parent, _)) = path.rsplit_once('/') {
            self.mkdir(parent);
        }
        self.write(path, content)
    }

    fn remove_file(&self, path: &str) -> Result<(), String> {
        if self.files.borrow_mut().remove(path).is_none() {
            return Err(format!("{path}: No such file or directory"));
        }
        self.removals.borrow_mut().push(path.to_string());
        Ok(())
    }
}

pub struct FixedClock;

impl BackupClock for FixedClock {
    fn backup_timestamp(&self) -> String {
        TS.to_string()
    }
}

pub fn deps<'a>(fs: &'a MemFs, clock: &'a FixedClock) -> CodeStyleDeps<'a> {
    CodeStyleDeps { fs, clock }
}
