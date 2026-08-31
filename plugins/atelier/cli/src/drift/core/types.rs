//! Domain types of the drift subsystem: the marker/path constants shared by
//! check and sync, the check report whose exit code *is* the CLI contract, and
//! the sync report that renders the `synced:` line.
//!
//! The constants live here as the single source of truth (the role
//! `drift-common.sh` played for the shell scripts): if check and sync each
//! carried their own copy, editing one side would silently desynchronize
//! judgement from writing.

/// Block markers in the user CLAUDE.md. Matched on **whole-line equality**
/// only, never as substrings — a prose mention of the marker text inside a
/// longer line must not toggle the block.
pub const BEGIN_MARKER: &str = "<!-- [coding-style:begin] DO NOT REMOVE THIS LINE -->";
pub const END_MARKER: &str = "<!-- [coding-style:end] DO NOT REMOVE THIS LINE -->";

/// Plugin-root-relative source paths (the template file itself contains both
/// markers) and the project-relative location of the rules copy.
pub const TEMPLATE_CLAUDE_MD_REL: &str = "templates/claude-md/CLAUDE.md";
pub const TEMPLATE_RULES_REL: &str = "rules/agent-design-principles.md";
pub const RULES_COPY_REL: &str = ".claude/rules/agent-design-principles.md";

/// User-scope rules: plugin sources under `rules/user/`, installed verbatim
/// into the user rules directory (default `~/.claude/rules/atelier`). The
/// manifest is an explicit list, not a directory scan, so what setup installs
/// and drift judges is pinned by the binary — a stray file in the source dir
/// never ships. Distributing a new policy file = add it here and under
/// `rules/user/`.
pub const TEMPLATE_USER_RULES_DIR_REL: &str = "rules/user";
pub const USER_RULES: &[&str] = &["plan-vs-spec.md"];

/// Check names as they appear on stdout — `commands/update.md` branches on
/// these exact strings. User-rules findings render as `user-rules/<file>`.
pub const CLAUDE_MD_CHECK: &str = "claude-md-coding-style-block";
pub const RULES_CHECK: &str = "rules/agent-design-principles.md";
pub const USER_RULES_CHECK_PREFIX: &str = "user-rules/";

/// Marker occurrences in a line sequence. One scan shared by check
/// (judgement) and sync (range replacement), so the two sides can never
/// disagree about where the block is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerScan {
    /// Index of the first begin/end marker line, `None` when absent.
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

/// A decoded artifact read. `NonUtf8` is a judgement input, not an error:
/// user-side artifacts may legitimately hold bytes the UTF-8 plugin sources
/// can never equal, and check must report that as drift instead of dying with
/// the exit-2 error contract reserved for plugin-source/usage failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactContent {
    Utf8(String),
    NonUtf8,
}

/// Which installed copy a `drift sync` run updates. A closed enum rather than
/// a string so an unknown target dies at the clap boundary (exit 2), never
/// deep inside the command.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncTarget {
    /// The `[coding-style]` marker range inside the user CLAUDE.md.
    ClaudeMd,
    /// The project-local rules copy.
    Rules,
    /// The user-scope rules copies (every `USER_RULES` file at once).
    UserRules,
}

/// The three roots every drift command derives its file paths from. Resolved
/// once at the CLI edge (defaults included), so the commands never consult the
/// environment themselves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriftPaths {
    /// Plugin root the source files live under.
    pub plugin_root: String,
    /// The user CLAUDE.md holding the coding-style block.
    pub claude_md: String,
    /// Project root the rules copy lives under.
    pub project_dir: String,
    /// Directory the user-scope rules copies live in.
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
    pub fn template_user_rule(&self, name: &str) -> String {
        format!(
            "{}/{}/{}",
            self.plugin_root, TEMPLATE_USER_RULES_DIR_REL, name
        )
    }
    pub fn user_rule_copy(&self, name: &str) -> String {
        format!("{}/{}", self.user_rules_dir, name)
    }
}

/// Per-artifact judgement. `NotInstalled` is deliberately not drift: update
/// never installs, so a missing artifact is a report, not a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactStatus {
    Ok,
    Drifted,
    NotInstalled,
}

impl ArtifactStatus {
    /// The stdout token — the strings `commands/update.md` branches on.
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactStatus::Ok => "OK",
            ArtifactStatus::Drifted => "DRIFTED",
            ArtifactStatus::NotInstalled => "NOT_INSTALLED",
        }
    }
}

/// One `<check>=<STATUS> [detail]` judgement line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckFinding {
    pub name: String,
    pub status: ArtifactStatus,
    /// Path or reason shown in parentheses; OK findings carry none.
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

/// The full `drift check` result. Rendering and the exit-code policy both
/// live on the type so the CLI edge only prints and returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub findings: Vec<CheckFinding>,
}

impl CheckReport {
    fn count(&self, status: ArtifactStatus) -> usize {
        self.findings.iter().filter(|f| f.status == status).count()
    }

    fn drifted(&self) -> usize {
        self.count(ArtifactStatus::Drifted)
    }

    /// Shell-compatible check contract: drift found → 1, otherwise 0
    /// (NOT_INSTALLED alone is not drift). Errors (exit 2) never reach a
    /// report — they surface as `Err` before one is built.
    pub fn exit_code(&self) -> i32 {
        if self.drifted() > 0 {
            1
        } else {
            0
        }
    }

    /// One line per finding plus the summary line, exactly the format the
    /// `/atelier:update` spec consumes.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for finding in &self.findings {
            out.push_str(&finding.render());
            out.push('\n');
        }
        out.push_str(&format!(
            "→ {} checked, {} drifted, {} missing\n",
            self.findings.len(),
            self.drifted(),
            self.count(ArtifactStatus::NotInstalled)
        ));
        out
    }
}

/// A completed `drift sync` write: which target, where, and the backup taken
/// before the overwrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub target: SyncTarget,
    pub path: String,
    pub backup: String,
}

impl SyncReport {
    /// The `synced:` stdout line, preserving the two shell formats.
    /// Newline-terminated like `CheckReport::render`, so the CLI edge prints
    /// both reports the same way.
    pub fn render(&self) -> String {
        match self.target {
            SyncTarget::ClaudeMd => format!(
                "synced: coding-style block in {} (backup: {})\n",
                self.path, self.backup
            ),
            SyncTarget::Rules | SyncTarget::UserRules => {
                format!("synced: {} (backup: {})\n", self.path, self.backup)
            }
        }
    }
}
