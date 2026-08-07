//! `setup guard` command — the one-shot installer for the branch-guard hooks.
//! It is deliberately *not* part of `guard`: that surface is the hook runtime
//! (reads stdin, exit 2 means "block"), whereas this one writes the
//! registration. Sharing them would make an install failure look like a block
//! to Claude Code, and would overload `--default-branch` to mean both "the pin
//! to produce" and "the pin to consume".
//!
//! The steps this collapses into one deterministic call — previously markdown
//! bash an LLM had to reproduce byte-for-byte on every setup:
//!
//! 1. warm `origin/HEAD` against the project repo (best effort),
//! 2. detect the default branch (forge first, then the guard's own read-only
//!    detection), absorbing every failure into "no pin",
//! 3. resolve the scope's settings file,
//! 4. retire stale guard registrations by command prefix,
//! 5. register both guards in a single write.

use crate::git::commands::hook::{settings_path, HookCommand};
use crate::git::core::git::{GitService, OriginHeadWarmer};
use crate::git::core::github::RepoDefaultBranch;
use crate::git::types::{
    CmdResult, DetectedBranch, GuardSetupOutput, HookRegisterManyInput, HookRegistration, HookScope,
};

/// Must reach settings.json **verbatim**: the shell expands it when the hook
/// fires, so setup expanding it would freeze one session's project path into
/// every future invocation.
const PROJECT_DIR_ARG: &str = r#"--project-dir "${CLAUDE_PROJECT_DIR:-.}""#;

/// Hook type both guards register under.
const HOOK_TYPE: &str = "PreToolUse";

/// The branch-guard targets setup installs, each with the tool matcher it
/// gates. `guard pr` is absent on purpose — it carries no project-specific
/// value, so the hook-management flow registers it separately.
const GUARD_TARGETS: [(&str, &str); 2] = [("write", "Write|Edit"), ("commit", "Bash")];

/// The only place a guard command string is produced. A `None` branch does not
/// push the flag at all — a bare `--default-branch` makes the hook exit 2 on
/// clap's usage error, which Claude Code reads as "deny every edit".
fn guard_command(target: &str, branch: Option<&DetectedBranch>) -> String {
    let mut command = format!("atelier git guard {target} {PROJECT_DIR_ARG}");
    if let Some(branch) = branch {
        command.push_str(" --default-branch ");
        command.push_str(branch.as_str());
    }
    command
}

/// Prefix identifying *any* generation of a registered branch guard, pinned or
/// not. Exact-command replacement cannot retire an entry whose trailing
/// `--default-branch <b>` differs from the command being registered, so without
/// this purge both would survive and the guard would run twice.
fn guard_command_prefix(target: &str) -> String {
    format!("atelier git guard {target} ")
}

pub struct GuardSetupDeps<'a> {
    pub warmer: &'a dyn OriginHeadWarmer,
    pub git: &'a dyn GitService,
    pub gh: &'a dyn RepoDefaultBranch,
    pub hook: &'a HookCommand<'a>,
}

pub struct GuardSetupInput {
    /// The repository the guards protect — also the anchor for the warm-up and
    /// for detection, which must not follow the process cwd.
    pub project_dir: String,
    pub scope: HookScope,
    pub dry_run: bool,
}

/// Forge answer first (authoritative, and the only source that knows a default
/// like `trunk` without a warmed ref), then the guard's own read-only
/// detection. Both failing collapses to `None`: setup registers unpinned rather
/// than aborting, and the warm-up above covers the runtime case.
fn detect_branch(deps: &GuardSetupDeps) -> Option<DetectedBranch> {
    deps.gh.default_branch().or_else(|| {
        deps.git
            .detect_default_branch()
            .ok()
            .as_deref()
            .and_then(DetectedBranch::new)
    })
}

pub fn run(deps: &GuardSetupDeps, input: &GuardSetupInput) -> CmdResult<GuardSetupOutput> {
    // 1. Warm-up runs first and unconditionally: even a scope that never pins
    //    benefits, because it is what lets the guard's runtime detection find a
    //    non-standard default. A failure here is reported, never fatal.
    let origin_head_warmed = deps.warmer.warm_origin_head();

    // 2. Only detect where a pin is allowed — under user scope the answer could
    //    not be used, and asking would spend a network round-trip to discard it.
    let branch = if input.scope.pins_default_branch() {
        detect_branch(deps)
    } else {
        None
    };

    // 3. Scope decides which settings.json is written.
    let settings_dir = match input.scope.settings_dir(&input.project_dir) {
        Ok(dir) => dir,
        Err(e) => return CmdResult::Err(e),
    };

    // 4 + 5. Purge stale generations and register both guards in one write, so
    //        the install can never land half-migrated or half-registered.
    let commands: Vec<String> = GUARD_TARGETS
        .iter()
        .map(|(target, _)| guard_command(target, branch.as_ref()))
        .collect();
    let batch = HookRegisterManyInput {
        hooks: GUARD_TARGETS
            .iter()
            .zip(&commands)
            .map(|((_, matcher), command)| HookRegistration {
                hook_type: HOOK_TYPE.to_string(),
                matcher: matcher.to_string(),
                command: command.clone(),
                timeout: None,
            })
            .collect(),
        remove_command_prefixes: GUARD_TARGETS
            .iter()
            .map(|(target, _)| guard_command_prefix(target))
            .collect(),
        project_dir: Some(settings_dir.clone()),
        dry_run: input.dry_run,
    };

    match deps.hook.register_many(&batch) {
        Ok(CmdResult::Ok(result)) => CmdResult::Ok(GuardSetupOutput {
            scope: input.scope,
            settings_path: settings_path(&settings_dir),
            default_branch: branch.map(|b| b.as_str().to_string()),
            origin_head_warmed,
            commands,
            removed: result.removed,
            dry_run: input.dry_run,
        }),
        Ok(CmdResult::Err(e)) | Err(e) => CmdResult::Err(e),
    }
}
