pub const SOURCE_ROOT_REL: &str = "rules/code-style";
pub const INSTALL_ROOT_REL: &str = ".claude/rules/atelier/code-style";

/// Source subdirectories holding one directory per language / app kind.
pub const LANG_SOURCE_DIR: &str = "lang";
pub const APP_SOURCE_DIR: &str = "app";

pub const RULE_EXT: &str = ".md";

/// Beside the group directories in the install root.
pub const RECORD_FILE: &str = "installed.json";

/// One installable set of rules. Every install brings `Base` along; it is
/// never a `--lang` or `--app` value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Base,
    Lang(String),
    App(String),
}

impl Group {
    /// The directory name the group installs under.
    pub fn name(&self) -> &str {
        match self {
            Group::Base => "base",
            Group::Lang(name) | Group::App(name) => name,
        }
    }
}

/// The languages and apps a command was asked to work on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    pub langs: Vec<String>,
    pub apps: Vec<String>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.langs.is_empty() && self.apps.is_empty()
    }
}

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
    pub fn lang_source_root(&self) -> String {
        format!("{}/{LANG_SOURCE_DIR}", self.source_root())
    }
    pub fn app_source_root(&self) -> String {
        format!("{}/{APP_SOURCE_DIR}", self.source_root())
    }
    pub fn source_dir(&self, group: &Group) -> String {
        match group {
            Group::Base => format!("{}/{}", self.source_root(), group.name()),
            Group::Lang(name) => format!("{}/{name}", self.lang_source_root()),
            Group::App(name) => format!("{}/{name}", self.app_source_root()),
        }
    }
    /// Installed groups sit side by side, whatever their kind.
    pub fn install_dir(&self, group: &Group) -> String {
        format!("{}/{}", self.install_root(), group.name())
    }
    pub fn record_path(&self) -> String {
        format!("{}/{RECORD_FILE}", self.install_root())
    }
}

/// The report name of one rule file — callers branch on this exact string.
pub fn rule_name(group: &Group, file: &str) -> String {
    format!("code-style/{}/{file}", group.name())
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
