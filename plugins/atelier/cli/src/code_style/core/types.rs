pub const SOURCE_ROOT_REL: &str = "rules/code-style";
pub const INSTALL_ROOT_REL: &str = ".claude/rules/atelier/code-style";

/// The group every language install brings along. It is a directory beside
/// the language directories, never a `--lang` value.
pub const COMMON_GROUP: &str = "common";

pub const RULE_EXT: &str = ".md";

/// Beside the group directories in the install root.
pub const RECORD_FILE: &str = "installed.json";

/// Inside each language source directory; never installed.
pub const PROFILE_FILE: &str = "profile.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeStylePaths {
    pub plugin_root: String,
    pub project_dir: String,
}

impl CodeStylePaths {
    pub fn source_root(&self) -> String {
        format!("{}/{SOURCE_ROOT_REL}", self.plugin_root)
    }
    pub fn install_root(&self) -> String {
        format!("{}/{INSTALL_ROOT_REL}", self.project_dir)
    }
    pub fn source_dir(&self, group: &str) -> String {
        format!("{}/{group}", self.source_root())
    }
    pub fn install_dir(&self, group: &str) -> String {
        format!("{}/{group}", self.install_root())
    }
    pub fn record_path(&self) -> String {
        format!("{}/{RECORD_FILE}", self.install_root())
    }
    pub fn profile_path(&self, lang: &str) -> String {
        format!("{}/{PROFILE_FILE}", self.source_dir(lang))
    }
}

/// The report name of one rule file — callers branch on this exact string.
pub fn rule_name(group: &str, file: &str) -> String {
    format!("code-style/{group}/{file}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleStatus {
    Ok,
    Drifted,
    New,
    Orphaned,
    /// Handed to the project once and deleted there since — left out on purpose.
    Excluded,
}

impl RuleStatus {
    /// The stdout token callers branch on.
    pub fn as_str(&self) -> &'static str {
        match self {
            RuleStatus::Ok => "OK",
            RuleStatus::Drifted => "DRIFTED",
            RuleStatus::New => "NEW",
            RuleStatus::Orphaned => "ORPHANED",
            RuleStatus::Excluded => "EXCLUDED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFinding {
    pub name: String,
    pub status: RuleStatus,
    pub detail: Option<String>,
}

impl RuleFinding {
    fn render(&self) -> String {
        match &self.detail {
            Some(detail) => format!("{}={} ({})", self.name, self.status.as_str(), detail),
            None => format!("{}={}", self.name, self.status.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub findings: Vec<RuleFinding>,
}

impl CheckReport {
    fn count(&self, status: RuleStatus) -> usize {
        self.findings.iter().filter(|f| f.status == status).count()
    }

    pub fn exit_code(&self) -> i32 {
        let actionable = |status: RuleStatus| match status {
            RuleStatus::Ok | RuleStatus::Excluded => false,
            RuleStatus::Drifted | RuleStatus::New | RuleStatus::Orphaned => true,
        };
        if self.findings.iter().any(|f| actionable(f.status)) {
            1
        } else {
            0
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for finding in &self.findings {
            out.push_str(&finding.render());
            out.push('\n');
        }
        out.push_str(&format!(
            "→ {} checked, {} drifted, {} new, {} orphaned, {} excluded\n",
            self.findings.len(),
            self.count(RuleStatus::Drifted),
            self.count(RuleStatus::New),
            self.count(RuleStatus::Orphaned),
            self.count(RuleStatus::Excluded)
        ));
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyAction {
    Installed,
    Synced { backup: String },
    Removed { backup: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedChange {
    pub path: String,
    pub action: ApplyAction,
}

impl AppliedChange {
    fn render(&self) -> String {
        match &self.action {
            ApplyAction::Installed => format!("installed: {}", self.path),
            ApplyAction::Synced { backup } => format!("synced: {} (backup: {backup})", self.path),
            ApplyAction::Removed { backup } => {
                format!("removed: {} (backup: {backup})", self.path)
            }
        }
    }
}

/// What a command changed. `unchanged` counts the copies already OK,
/// `excluded` the rules the project left out and that stayed out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub changes: Vec<AppliedChange>,
    pub unchanged: usize,
    pub excluded: usize,
}

impl ApplyReport {
    fn count(&self, pick: fn(&ApplyAction) -> bool) -> usize {
        self.changes.iter().filter(|c| pick(&c.action)).count()
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for change in &self.changes {
            out.push_str(&change.render());
            out.push('\n');
        }
        out.push_str(&format!(
            "→ {} installed, {} synced, {} removed, {} unchanged, {} excluded\n",
            self.count(|a| matches!(a, ApplyAction::Installed)),
            self.count(|a| matches!(a, ApplyAction::Synced { .. })),
            self.count(|a| matches!(a, ApplyAction::Removed { .. })),
            self.unchanged,
            self.excluded
        ));
        out
    }
}
