//! Default-branch guard — port of `git-utils/src/core/guard.ts`. Decides
//! whether a write/commit on a protected branch is allowed. `GuardService`
//! takes a `GitService` by injection so it is unit-testable with a mock git.

use crate::git::core::bash_classifier::{
    classify, Anchor, BypassHit, BypassRule, ClassifyInput, Hit, HitKind, OpaqueCause,
};
use crate::git::core::git::GitService;
use crate::git::types::{GuardInput, GuardOutput, GuardTarget, GuardVerdict, OpaqueExecPolicy};
use regex::Regex;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

static GIT_COMMIT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bgit\b.*\bcommit\b").unwrap());

/// Branch-creation command shown in the block message when the caller passes no
/// `--create-branch-script`. The default lives with the guard (the code that
/// renders it), not the CLI router that merely forwards the flag.
pub const DEFAULT_CREATE_BRANCH_SCRIPT: &str = "git switch -c";

/// Replaces single-/double-quoted segments with a space so quoted text
/// arguments can't false-positive the commit matcher — on a protected branch,
/// `gh issue create --body "... git commit ..."` must not be treated as a
/// commit (#754). Trade-off: a commit nested entirely inside quotes
/// (`bash -c "git commit"`) is no longer matched; the guard is a guard-rail,
/// not an escape-proof sandbox.
fn strip_quoted(command: &str) -> String {
    let mut out = String::with_capacity(command.len());
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match c {
            // An escaped char never opens/closes a quote; keep it verbatim so
            // `echo \"git commit\"` stays conservative (still matches).
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '\'' => {
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                }
                out.push(' ');
            }
            '"' => {
                while let Some(q) = chars.next() {
                    if q == '\\' {
                        chars.next();
                    } else if q == '"' {
                        break;
                    }
                }
                out.push(' ');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Lexically collapses `.`/`..` in `path` (relative to `base`) without touching
/// the filesystem. A relative `path` is anchored at `base` rather than the
/// process cwd — the guard runs as a PreToolUse hook whose cwd may differ from
/// the project (worktree / subagent contexts), so resolving against cwd
/// mis-judges relative `file_path`s (#780).
fn resolve_against(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    let mut out = if path.is_absolute() {
        PathBuf::new()
    } else {
        base.to_path_buf()
    };
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolves `project_dir` itself, anchoring a relative project dir at the
/// process cwd (the project dir is the anchor, so there is no better base).
fn resolve_project_dir(project_dir: &str) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    resolve_against(&cwd, project_dir)
}

/// Port of TS `isInsideProjectDir`: true when `file_path` is the project dir
/// itself or strictly inside it (no `..` escape, not a sibling prefix match).
/// Relative `file_path`s are resolved against `project_dir` (#780).
pub fn is_inside_project_dir(file_path: &str, project_dir: &str) -> bool {
    inside_project_dir(&resolve_project_dir(project_dir), file_path)
}

fn inside_project_dir(project: &Path, file_path: &str) -> bool {
    let file = resolve_against(project, file_path);
    match file.strip_prefix(project) {
        Ok(rel) => {
            // rel == '' (same dir) or a normal relative descendant.
            rel.as_os_str().is_empty() || !rel.starts_with("..")
        }
        Err(_) => false,
    }
}

/// Port of TS `isInsideAnyGitRepo`: walks up from the file's directory looking
/// for a `.git` entry, skipping non-existent leading directories first.
/// Relative `file_path`s are resolved against `project_dir`, not the process
/// cwd, so the walk starts inside the project (#780).
pub fn is_inside_any_git_repo(file_path: &str, project_dir: &str) -> bool {
    inside_any_git_repo(&resolve_project_dir(project_dir), file_path)
}

fn inside_any_git_repo(project: &Path, file_path: &str) -> bool {
    let resolved = resolve_against(project, file_path);
    has_git_ancestor(resolved.parent().map(PathBuf::from).unwrap_or(resolved))
}

/// True when `start` or one of its ancestors holds a `.git` entry. A `start`
/// that does not exist yet is first lifted to its nearest existing ancestor.
fn has_git_ancestor(start: PathBuf) -> bool {
    let mut dir = start;
    let root = PathBuf::from("/");

    // Skip up to the first existing ancestor.
    while dir != root && !dir.exists() {
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => break,
        }
    }
    // Walk up looking for .git.
    while dir != root {
        if dir.join(".git").exists() {
            return true;
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => break,
        }
    }
    false
}

pub trait GuardService {
    fn check(&self, input: &GuardInput) -> GuardOutput;
}

pub struct RealGuardService<'a> {
    git: &'a dyn GitService,
}

/// Constructs a guard service over the given git service.
pub fn create_guard_service(git: &dyn GitService) -> RealGuardService<'_> {
    RealGuardService { git }
}

impl GuardService for RealGuardService<'_> {
    fn check(&self, input: &GuardInput) -> GuardOutput {
        let pass = |reason: Option<&str>| GuardOutput {
            verdict: GuardVerdict::Allow,
            reason: reason.map(|s| s.to_string()),
            current_branch: None,
            default_branch: None,
        };

        // Bash hits that survived the prefilter; empty for the write target.
        let mut hits: Vec<Hit> = Vec::new();

        // Target-specific prefilters; the payload lives on the variant (#777).
        match &input.target {
            // write guard: file outside the project directory.
            GuardTarget::Write { file_path } => {
                if let Some(file_path) = file_path {
                    // Resolve project_dir once for both path checks (one
                    // current_dir() syscall instead of two per tool invocation).
                    let project = resolve_project_dir(&input.project_dir);
                    if !inside_project_dir(&project, file_path)
                        && !inside_any_git_repo(&project, file_path)
                    {
                        return pass(Some("file is outside any git repository"));
                    }
                }
            }
            GuardTarget::Commit { command, cwd } => {
                let Some(command) = command.as_deref().filter(|c| !c.is_empty()) else {
                    return pass(Some("no command to inspect"));
                };
                let project = resolve_project_dir(&input.project_dir);
                let cwd = cwd
                    .as_deref()
                    .map_or_else(|| project.clone(), |c| resolve_against(&project, c));
                let analysis = classify(&ClassifyInput {
                    command: command.to_string(),
                    cwd: cwd.clone(),
                    home: input.home.as_deref().map(PathBuf::from),
                });
                match analysis {
                    Ok(analysis) => {
                        if let Some(bypass) = analysis.bypass.first() {
                            return GuardOutput {
                                verdict: GuardVerdict::Block,
                                reason: Some(bypass_reason(bypass)),
                                current_branch: None,
                                default_branch: None,
                            };
                        }
                        hits = analysis.hits;
                    }
                    Err(_) => hits = unparsed_command_hits(command, &cwd),
                }
                hits.retain(|h| anchor_in_scope(&project, &h.anchor));
                if hits.is_empty() {
                    return pass(Some("no repository-changing command"));
                }
            }
        }

        // Guard 1: inside a git repo.
        if !self.git.is_inside_work_tree() {
            return pass(Some("not a git repository"));
        }

        // Resolve default branch. An empty/whitespace value (e.g. a setup that
        // detected nothing and recorded a bare `--default-branch`) is treated as
        // absence — not a real branch named "" — so the guard falls back to
        // detection and still protects the true default instead of silently
        // protecting nothing.
        let default_branch = match input.default_branch.as_deref().map(str::trim) {
            Some(b) if !b.is_empty() => b.to_string(),
            // Read-only detection — the guard must not mutate repo state
            // (no `git remote set-head`) on every tool invocation (#779).
            _ => match self.git.detect_default_branch() {
                Ok(b) => b,
                Err(_) => return pass(Some("could not detect default branch")),
            },
        };

        // Protected set: default + develop + extras.
        let mut protected: Vec<String> = vec![default_branch.clone(), "develop".to_string()];
        if let Some(extras) = &input.protected_branches {
            for b in extras {
                if !protected.contains(b) {
                    protected.push(b.clone());
                }
            }
        }

        // Guard 2: special state (rebase/merge) → pass. The snapshot also
        // carries the current branch, so guards 2–3 and the branch check cost
        // one `get_special_state` round-trip, not a second subprocess (#778).
        let state = self.git.get_special_state();
        if state.rebase || state.merge {
            return pass(Some("special git state (rebase/merge)"));
        }

        // Guard 3: detached HEAD → pass.
        if state.detached() {
            return pass(Some("detached HEAD"));
        }

        let current_branch = state.current_branch;

        if !protected.contains(&current_branch) {
            return GuardOutput {
                verdict: GuardVerdict::Allow,
                reason: None,
                current_branch: Some(current_branch),
                default_branch: Some(default_branch),
            };
        }

        // Fall back to the default command when no script was supplied, so the
        // CLI router can forward the raw `Option` without embedding the policy.
        let script = match input.create_branch_script.trim() {
            "" => DEFAULT_CREATE_BRANCH_SCRIPT,
            s => s,
        };
        let (verdict, reason) = match definite_action(&input.target, &hits) {
            Some(action) => (
                GuardVerdict::Block,
                [
                    format!("[Branch Guard] 보호 브랜치({current_branch})에서 {action}."),
                    "먼저 새 브랜치를 생성해주세요:".to_string(),
                    format!("  {script} <branch-name>"),
                ]
                .join("\n"),
            ),
            None => match input.opaque_exec {
                OpaqueExecPolicy::Allow => {
                    return GuardOutput {
                        verdict: GuardVerdict::Allow,
                        reason: Some("opaque execution allowed by --opaque-exec".to_string()),
                        current_branch: Some(current_branch),
                        default_branch: Some(default_branch),
                    };
                }
                policy => {
                    let program = hits.first().map_or("", |h| h.program.as_str());
                    opaque_reason(policy, &current_branch, program, script)
                }
            },
        };

        GuardOutput {
            verdict,
            reason: Some(reason),
            current_branch: Some(current_branch),
            default_branch: Some(default_branch),
        }
    }
}

/// The action to name in the block message when the target is certain to
/// change the repo: the write target, or a commit/write hit with a resolved
/// path. `None` means every remaining hit is uninspectable.
fn definite_action(target: &GuardTarget, hits: &[Hit]) -> Option<&'static str> {
    if matches!(target, GuardTarget::Write { .. }) {
        return Some("파일을 수정하려 합니다");
    }
    hits.iter()
        .filter(|h| matches!(h.anchor, Anchor::Path(_)))
        .find_map(|h| match h.kind {
            HitKind::Commit => Some("커밋할 수 없습니다"),
            HitKind::Write(_) => Some("파일을 수정하려 합니다"),
            HitKind::Opaque(_) => None,
        })
}

fn opaque_reason(
    policy: OpaqueExecPolicy,
    branch: &str,
    program: &str,
    script: &str,
) -> (GuardVerdict, String) {
    let (verdict, head) = match policy {
        OpaqueExecPolicy::Block => (
            GuardVerdict::Block,
            format!(
                "[Branch Guard] 보호 브랜치({branch})에서 `{program}` 의 스크립트/대상 내부를 확인할 수 없어 차단합니다(--opaque-exec block)."
            ),
        ),
        _ => (
            GuardVerdict::Ask,
            format!(
                "[Branch Guard] 보호 브랜치({branch})에서 `{program}` 의 스크립트/대상 내부를 확인할 수 없어 확인을 요청합니다. 저장소 파일을 바꾸지 않는다면 승인하세요."
            ),
        ),
    };
    let reason = [
        head,
        "파일을 바꿔야 한다면 먼저 새 브랜치를 생성해주세요:".to_string(),
        format!("  {script} <branch-name>"),
    ]
    .join("\n");
    (verdict, reason)
}

fn bypass_reason(hit: &BypassHit) -> String {
    let rule = match hit.rule {
        BypassRule::NoVerifyFlag => "--no-verify",
        BypassRule::CommitShortNoVerify => "commit -n",
        BypassRule::HooksPathConfig => "core.hooksPath",
        BypassRule::HooksPathEnv => "core.hooksPath env",
        BypassRule::HookSkipEnv => "hook skip env",
    };
    format!(
        "[Hook Guard] git hook 우회({rule}: {})는 프로젝트 정책상 금지입니다(브랜치 무관). hook 실패 원인을 수정한 뒤 다시 실행하세요.",
        hit.token
    )
}

/// Hits for a command the lexer could not read: the plain regex commit check
/// plus an opaque hit, so a protected branch never ends in a silent allow.
fn unparsed_command_hits(command: &str, cwd: &Path) -> Vec<Hit> {
    let anchor = Anchor::Path(cwd.to_path_buf());
    let mut hits = Vec::new();
    if GIT_COMMIT_PATTERN.is_match(&strip_quoted(command)) {
        hits.push(Hit {
            kind: HitKind::Commit,
            anchor: anchor.clone(),
            program: "git".to_string(),
        });
    }
    hits.push(Hit {
        kind: HitKind::Opaque(OpaqueCause::DynamicCommand),
        anchor,
        program: command.split_whitespace().next().unwrap_or("").to_string(),
    });
    hits
}

/// Whether a hit's anchor lands in the project or in any git repo. An
/// unresolved anchor without a literal prefix is taken to be inside the project.
fn anchor_in_scope(project: &Path, anchor: &Anchor) -> bool {
    let target = match anchor {
        Anchor::Path(p) => p,
        Anchor::Unresolved {
            literal_prefix: Some(p),
            ..
        } => p,
        Anchor::Unresolved {
            literal_prefix: None,
            ..
        } => return true,
    };
    target
        .strip_prefix(project)
        .is_ok_and(|rel| !rel.starts_with(".."))
        || has_git_ancestor(target.clone())
}
