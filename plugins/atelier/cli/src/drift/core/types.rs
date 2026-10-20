pub const BEGIN_MARKER: &str = "<!-- [coding-style:begin] DO NOT REMOVE THIS LINE -->";
pub const END_MARKER: &str = "<!-- [coding-style:end] DO NOT REMOVE THIS LINE -->";

pub const TEMPLATE_CLAUDE_MD_REL: &str = "templates/claude-md/CLAUDE.md";
pub const TEMPLATE_RULES_REL: &str = "rules/agent-design-principles.md";
pub const RULES_COPY_REL: &str = ".claude/rules/agent-design-principles.md";

/// An explicit list, not a directory scan: shipping a new policy file means
/// adding it both here and under `rules/policies/`.
pub const TEMPLATE_POLICY_RULES_DIR_REL: &str = "rules/policies";
pub const POLICY_RULES: &[&str] = &[
    "spec-writing.md",
    "rules-writing.md",
    "plan-writing.md",
    "doc-dependency.md",
];

pub const PROJECT_RULES_DIR_REL: &str = ".claude/rules/atelier";

/// Callers branch on these exact strings.
pub const CLAUDE_MD_CHECK: &str = "claude-md-coding-style-block";
pub const RULES_CHECK: &str = "rules/agent-design-principles.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerScan {
    /// Index of the first begin/end marker line.
    pub begin: Option<usize>,
    pub end: Option<usize>,
    pub begin_count: usize,
    pub end_count: usize,
}

/// Scans `lines` for the markers — whole-line equality only, so a prose
/// mention of the marker text inside a longer line never counts.
pub fn scan_markers(lines: &[&str]) -> MarkerScan {
    let mut scan = MarkerScan {
        begin: None,
        end: None,
        begin_count: 0,
        end_count: 0,
    };
    for (idx, line) in lines.iter().enumerate() {
        if *line == BEGIN_MARKER {
            if scan.begin.is_none() {
                scan.begin = Some(idx);
            }
            scan.begin_count += 1;
        } else if *line == END_MARKER {
            if scan.end.is_none() {
                scan.end = Some(idx);
            }
            scan.end_count += 1;
        }
    }
    scan
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactContent {
    Utf8(String),
    NonUtf8,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncTarget {
    /// The `[coding-style]` marker range inside the user CLAUDE.md.
    ClaudeMd,
    /// The project-local rules copy.
    Rules,
    /// The user-scope policy copies (`POLICY_RULES`, or one via `--name`).
    UserRules,
    /// The project-scope policy copies (`POLICY_RULES`, or one via `--name`).
    ProjectRules,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftPaths {
    pub plugin_root: String,
    pub claude_md: String,
    pub project_dir: String,
    pub user_rules_dir: String,
}

impl DriftPaths {
    pub fn template_claude_md(&self) -> String {
        format!("{}/{}", self.plugin_root, TEMPLATE_CLAUDE_MD_REL)
    }
    pub fn template_rules(&self) -> String {
        format!("{}/{}", self.plugin_root, TEMPLATE_RULES_REL)
    }
    pub fn rules_copy(&self) -> String {
        format!("{}/{}", self.project_dir, RULES_COPY_REL)
    }
    pub fn template_policy_rule(&self, name: &str) -> String {
        format!(
            "{}/{}/{}",
            self.plugin_root, TEMPLATE_POLICY_RULES_DIR_REL, name
        )
    }
    pub fn user_rule_copy(&self, name: &str) -> String {
        format!("{}/{}", self.user_rules_dir, name)
    }
    pub fn project_rule_copy(&self, name: &str) -> String {
        format!("{}/{}/{}", self.project_dir, PROJECT_RULES_DIR_REL, name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactStatus {
    Ok,
    Drifted,
    New,
    NotInstalled,
}

impl ArtifactStatus {
    /// The stdout token callers branch on.
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactStatus::Ok => "OK",
            ArtifactStatus::Drifted => "DRIFTED",
            ArtifactStatus::New => "NEW",
            ArtifactStatus::NotInstalled => "NOT_INSTALLED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckFinding {
    pub name: String,
    pub status: ArtifactStatus,
    pub detail: Option<String>,
}

impl CheckFinding {
    fn render(&self) -> String {
        match &self.detail {
            Some(detail) => format!("{}={} ({})", self.name, self.status.as_str(), detail),
            None => format!("{}={}", self.name, self.status.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub findings: Vec<CheckFinding>,
}

impl CheckReport {
    fn count(&self, status: ArtifactStatus) -> usize {
        self.findings.iter().filter(|f| f.status == status).count()
    }

    pub fn exit_code(&self) -> i32 {
        if self.count(ArtifactStatus::Drifted) + self.count(ArtifactStatus::New) > 0 {
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
            "→ {} checked, {} drifted, {} new, {} missing\n",
            self.findings.len(),
            self.count(ArtifactStatus::Drifted),
            self.count(ArtifactStatus::New),
            self.count(ArtifactStatus::NotInstalled)
        ));
        out
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub target: SyncTarget,
    pub path: String,
    pub backup: Option<String>,
}

impl SyncReport {
    pub fn render(&self) -> String {
        let Some(backup) = &self.backup else {
            return format!("installed: {}\n", self.path);
        };
        match self.target {
            SyncTarget::ClaudeMd => format!(
                "synced: coding-style block in {} (backup: {backup})\n",
                self.path
            ),
            SyncTarget::Rules | SyncTarget::UserRules | SyncTarget::ProjectRules => {
                format!("synced: {} (backup: {backup})\n", self.path)
            }
        }
    }
}
