//! Shared test doubles and fixtures for the drift subsystem tests, following
//! the same convention as `session_mocks`. The filesystem and the backup clock
//! are in-memory, so the drift rules are pinned without touching a real
//! CLAUDE.md or a temp directory — and the write log lets refusal tests assert
//! that a rejected sync performed *zero* writes.
//!
//! `#![allow(dead_code)]` because each test binary uses only a subset (Cargo
//! compiles this module separately into every test crate).
#![allow(dead_code)]

use atelier::drift::commands::DriftDeps;
use atelier::drift::core::artifact::{ArtifactFs, BackupClock};
use atelier::drift::core::types::{
    ArtifactContent, DriftPaths, BEGIN_MARKER, END_MARKER, POLICY_RULES,
};
use std::cell::RefCell;
use std::collections::HashMap;

/// Fixed timestamp the `FixedClock` hands out, so backup paths are stable.
pub const TS: &str = "20260821-120000";

pub const PLUGIN_ROOT: &str = "/plugin";
pub const USER_CLAUDE_MD: &str = "/home/u/.claude/CLAUDE.md";
pub const PROJECT_DIR: &str = "/proj";
pub const USER_RULES_DIR: &str = "/home/u/.claude/rules/atelier";

/// Paths the commands derive from the fixture roots above. The three policy
/// constants are the paths of `POLICY_RULES[0]`, for tests that target one
/// file.
pub const TEMPLATE_CLAUDE_MD: &str = "/plugin/templates/claude-md/CLAUDE.md";
pub const TEMPLATE_RULES: &str = "/plugin/rules/agent-design-principles.md";
pub const RULES_COPY: &str = "/proj/.claude/rules/agent-design-principles.md";
pub const TEMPLATE_USER_RULE: &str = "/plugin/rules/policies/spec-writing.md";
pub const USER_RULE_COPY: &str = "/home/u/.claude/rules/atelier/spec-writing.md";
pub const PROJECT_RULE_COPY: &str = "/proj/.claude/rules/atelier/spec-writing.md";

/// Fixture-root paths of one policy manifest entry.
pub fn template_policy_rule(name: &str) -> String {
    format!("{PLUGIN_ROOT}/rules/policies/{name}")
}
pub fn user_rule_copy(name: &str) -> String {
    format!("{USER_RULES_DIR}/{name}")
}
pub fn project_rule_copy(name: &str) -> String {
    format!("{PROJECT_DIR}/.claude/rules/atelier/{name}")
}

/// Canonical rules source bodies used by the fixtures.
pub const RULES_BODY: &str = "# Agent design principles\n\n- keep CLI deterministic\n";
pub const USER_RULE_BODY: &str = "# Policy rule\n\n- plan is history, spec is policy\n";

/// A coding-style block exactly as the template file ships it: the markers are
/// part of the template itself.
pub fn block(body: &str) -> String {
    format!("{BEGIN_MARKER}\n{body}\n{END_MARKER}\n")
}

/// The standard argument set pointing at the fixture roots.
pub fn paths() -> DriftPaths {
    DriftPaths {
        plugin_root: PLUGIN_ROOT.to_string(),
        claude_md: USER_CLAUDE_MD.to_string(),
        project_dir: PROJECT_DIR.to_string(),
        user_rules_dir: USER_RULES_DIR.to_string(),
    }
}

/// In-memory `ArtifactFs`. `writes` records every write in call order, so a
/// refusal path can assert nothing was touched and a success path can inspect
/// the backup that was captured before the overwrite. Entries are bytes so
/// fixtures can hold non-UTF-8 files, mirroring the real filesystem.
#[derive(Default)]
pub struct MemFs {
    pub files: RefCell<HashMap<String, Vec<u8>>>,
    pub writes: RefCell<Vec<(String, String)>>,
}

impl MemFs {
    pub fn insert(&self, path: &str, content: &str) {
        self.insert_bytes(path, content.as_bytes());
    }

    pub fn insert_bytes(&self, path: &str, bytes: &[u8]) {
        self.files
            .borrow_mut()
            .insert(path.to_string(), bytes.to_vec());
    }

    /// Fresh filesystem holding every plugin source file (template block body
    /// `tpl_body`, rules source `RULES_BODY`, every `POLICY_RULES` source as
    /// `USER_RULE_BODY`) and nothing else.
    pub fn with_sources(tpl_body: &str) -> Self {
        let fs = MemFs::default();
        fs.insert(TEMPLATE_CLAUDE_MD, &block(tpl_body));
        fs.insert(TEMPLATE_RULES, RULES_BODY);
        for name in POLICY_RULES {
            fs.insert(&template_policy_rule(name), USER_RULE_BODY);
        }
        fs
    }

    /// Installs every user-scope policy copy in sync with its source.
    pub fn install_user_rule_copies(&self) {
        for name in POLICY_RULES {
            self.insert(&user_rule_copy(name), USER_RULE_BODY);
        }
    }

    /// Installs every project-scope policy copy in sync with its source.
    pub fn install_project_rule_copies(&self) {
        for name in POLICY_RULES {
            self.insert(&project_rule_copy(name), USER_RULE_BODY);
        }
    }

    pub fn content(&self, path: &str) -> Option<String> {
        self.files
            .borrow()
            .get(path)
            .and_then(|bytes| String::from_utf8(bytes.clone()).ok())
    }

    pub fn write_count(&self) -> usize {
        self.writes.borrow().len()
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
        self.writes
            .borrow_mut()
            .push((path.to_string(), content.to_string()));
        self.insert(path, content);
        Ok(())
    }
}

/// Deterministic `BackupClock` — always answers `TS`.
pub struct FixedClock;

impl BackupClock for FixedClock {
    fn backup_timestamp(&self) -> String {
        TS.to_string()
    }
}

/// Assembles command deps from the doubles.
pub fn deps<'a>(fs: &'a MemFs, clock: &'a FixedClock) -> DriftDeps<'a> {
    DriftDeps { fs, clock }
}
