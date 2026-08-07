//! Type definitions for the git subsystem — a faithful Rust port of
//! `git-utils/src/types.ts`. The `Result<T>` discriminated union and all
//! Input/Output structs preserve the original JSON shapes so `atelier git`
//! emits byte-identical output to the legacy `git-utils` CLI.

use serde::Serialize;

/// Mirror of the TS `Result<T> = {ok:true,data} | {ok:false,error}`. Commands
/// return this; the CLI layer serializes `data` on success and prints
/// `Error: <error>` to stderr (exit 1) on failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdResult<T> {
    Ok(T),
    Err(String),
}

impl<T> CmdResult<T> {
    pub fn is_ok(&self) -> bool {
        matches!(self, CmdResult::Ok(_))
    }
}

// ---------------------------------------------------------------------------
// Reviews
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ReviewsInput {
    pub pr_number: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewComment {
    pub author: String,
    pub body: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewThread {
    #[serde(rename = "isResolved")]
    pub is_resolved: bool,
    #[serde(rename = "isOutdated")]
    pub is_outdated: bool,
    pub path: String,
    pub line: i64,
    pub comments: Vec<ReviewComment>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReviewsOutput {
    #[serde(rename = "prTitle")]
    pub pr_title: String,
    #[serde(rename = "prUrl")]
    pub pr_url: String,
    pub threads: Vec<ReviewThread>,
}

// ---------------------------------------------------------------------------
// Guard
// ---------------------------------------------------------------------------

/// Branch-guard target as a data-bearing enum: each variant carries exactly
/// the tool payload its check consumes, so invalid combinations (e.g. a write
/// guard with a tool command) are unrepresentable (#777).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardTarget {
    Write { file_path: Option<String> },
    Commit { command: Option<String> },
}

/// Full guard surface the CLI dispatches on. Branch targets route to the
/// branch guard (`GuardService`), `Pr` routes to the PR duplicate guard
/// (`PrGuardService`) — the branch guard never sees a `Pr` target by type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardCommandTarget {
    Branch(GuardTarget),
    Pr { command: Option<String> },
}

#[derive(Debug, Clone)]
pub struct GuardInput {
    pub target: GuardTarget,
    pub project_dir: String,
    pub create_branch_script: String,
    pub default_branch: Option<String>,
    pub protected_branches: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardOutput {
    pub allowed: bool,
    pub reason: Option<String>,
    pub current_branch: Option<String>,
    pub default_branch: Option<String>,
}

/// Unified allow/block decision returned by the guard command after routing
/// a target to its service — the only contract the CLI exit mapping needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardDecision {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl GuardDecision {
    /// PreToolUse hook exit contract: allow → 0, block → 2 (deny signal).
    /// Lives on the decision type so the CLI edge only prints and returns.
    pub fn exit_code(&self) -> i32 {
        if self.allowed {
            0
        } else {
            2
        }
    }
}

impl From<GuardOutput> for GuardDecision {
    fn from(out: GuardOutput) -> Self {
        GuardDecision {
            allowed: out.allowed,
            reason: out.reason,
        }
    }
}

impl From<PrGuardOutput> for GuardDecision {
    fn from(out: PrGuardOutput) -> Self {
        GuardDecision {
            allowed: out.allowed,
            reason: out.reason,
        }
    }
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct HookRegisterInput {
    pub hook_type: String,
    pub matcher: String,
    pub command: String,
    pub timeout: Option<i64>,
    pub project_dir: Option<String>,
}

/// One hook registration inside a batch. Carries no `project_dir`: the
/// settings file is a property of the batch, not of the individual hook.
#[derive(Debug, Clone)]
pub struct HookRegistration {
    pub hook_type: String,
    pub matcher: String,
    pub command: String,
    pub timeout: Option<i64>,
}

/// A batch of hook mutations applied in a single read-modify-write. Registering
/// N hooks with N `register` calls means N writes, so a failure midway leaves
/// settings.json half-registered; the batch collapses that to one write.
#[derive(Debug, Clone, Default)]
pub struct HookRegisterManyInput {
    /// Applied after the prefix purge, in order.
    pub hooks: Vec<HookRegistration>,
    /// Commands starting with any of these are dropped from every hook type
    /// before the registrations are applied. `register` replaces a hook by
    /// *exact* command match, which cannot retire an older registration whose
    /// trailing flags differ — the purge is the migration lever for that.
    pub remove_command_prefixes: Vec<String>,
    pub project_dir: Option<String>,
    /// Compute the result but skip the write, so a caller can show the planned
    /// change before touching a shared settings.json.
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookRegisterManyOutput {
    pub registered: Vec<HookRegisterOutput>,
    /// Commands dropped by the prefix purge, in discovery order.
    pub removed: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HookUnregisterInput {
    pub hook_type: String,
    pub command: String,
    pub project_dir: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HookListInput {
    pub hook_type: Option<String>,
    pub project_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookRegisterOutput {
    pub action: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HookUnregisterOutput {
    pub command: String,
}

// ---------------------------------------------------------------------------
// Guard setup
// ---------------------------------------------------------------------------

/// A default-branch name known to be non-blank. The empty string being
/// unrepresentable is what keeps a *bare* `--default-branch` out of
/// settings.json: the flag is only ever emitted from a `DetectedBranch`, and a
/// value-less flag would make the guard hook fail clap parsing with exit 2 —
/// which Claude Code reads as "block", denying every edit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DetectedBranch(String);

impl DetectedBranch {
    /// Sole entry point: trims, and treats a blank result as absence rather
    /// than as a branch literally named "".
    pub fn new(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| DetectedBranch(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Which settings.json a hook registration targets.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HookScope {
    User,
    Project,
}

impl HookScope {
    /// Directory whose `.claude/settings.json` this scope writes: user scope
    /// resolves `$HOME`, project scope stays on the project itself.
    pub fn settings_dir(&self, project_dir: &str) -> Result<String, String> {
        match self {
            HookScope::User => std::env::var("HOME")
                .map_err(|_| "HOME is not set — cannot resolve the user scope".to_string()),
            HookScope::Project => Ok(project_dir.to_string()),
        }
    }

    /// Whether a detected default branch may be baked into the guard command.
    /// User scope must not: one global pin forces a single repository's default
    /// branch onto every project (#810). The setup-time `origin/HEAD` warm-up
    /// lets the guard's read-only detection resolve non-standard defaults at
    /// runtime instead, so dropping the pin costs no protection.
    pub fn pins_default_branch(&self) -> bool {
        matches!(self, HookScope::Project)
    }
}

/// What `setup guard` did (or, under `--dry-run`, would do). `commands` is the
/// registered form, so callers can show the exact strings without re-reading
/// settings.json — and a reviewer can see at a glance whether a pin was added.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GuardSetupOutput {
    pub scope: HookScope,
    #[serde(rename = "settingsPath")]
    pub settings_path: String,
    /// `None` when nothing was pinned — either detection failed or the scope
    /// forbids pinning.
    #[serde(rename = "defaultBranch")]
    pub default_branch: Option<String>,
    #[serde(rename = "originHeadWarmed")]
    pub origin_head_warmed: bool,
    pub commands: Vec<String>,
    /// Stale guard registrations retired by the prefix purge (migration).
    pub removed: Vec<String>,
    #[serde(rename = "dryRun")]
    pub dry_run: bool,
}

// ---------------------------------------------------------------------------
// PR Guard
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct PrGuardInput {
    pub tool_command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrGuardOutput {
    pub allowed: bool,
    pub reason: Option<String>,
    pub pr_number: Option<i64>,
}

// ---------------------------------------------------------------------------
// Git special state — shared by core modules
// ---------------------------------------------------------------------------

/// Special-state snapshot taken in a single round-trip: carrying the current
/// branch here lets the guard read rebase/merge/detached *and* the branch from
/// one `get_special_state` call instead of a second subprocess (#778).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSpecialState {
    pub rebase: bool,
    pub merge: bool,
    pub current_branch: String,
}

impl GitSpecialState {
    /// Detached HEAD — `git branch --show-current` prints nothing.
    pub fn detached(&self) -> bool {
        self.current_branch.is_empty()
    }
}
