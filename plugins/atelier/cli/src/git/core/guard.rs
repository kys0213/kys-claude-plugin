//! Default-branch guard — port of `git-utils/src/core/guard.ts`. Decides
//! whether a write/commit on a protected branch is allowed. `GuardService`
//! takes a `GitService` by injection so it is unit-testable with a mock git.

use crate::git::core::git::GitService;
use crate::git::types::{GuardInput, GuardOutput, GuardTarget};
use regex::Regex;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

static GIT_COMMIT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bgit\b.*\bcommit\b").unwrap());

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
    let mut dir = resolved.parent().map(PathBuf::from).unwrap_or(resolved);
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
            allowed: true,
            reason: reason.map(|s| s.to_string()),
            current_branch: None,
            default_branch: None,
        };

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
            // commit guard: not a git commit command → pass. Quoted segments
            // are stripped so text arguments mentioning "git commit" don't
            // match (#754).
            GuardTarget::Commit { command } => {
                let is_commit = command
                    .as_ref()
                    .map(|c| !c.is_empty() && GIT_COMMIT_PATTERN.is_match(&strip_quoted(c)))
                    .unwrap_or(false);
                if !is_commit {
                    return pass(Some("not a git commit command"));
                }
            }
        }

        // Guard 1: inside a git repo.
        if !self.git.is_inside_work_tree() {
            return pass(Some("not a git repository"));
        }

        // Resolve default branch.
        let default_branch = match &input.default_branch {
            Some(b) => b.clone(),
            // Read-only detection — the guard must not mutate repo state
            // (no `git remote set-head`) on every tool invocation (#779).
            None => match self.git.detect_default_branch_readonly() {
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
                allowed: true,
                reason: None,
                current_branch: Some(current_branch),
                default_branch: Some(default_branch),
            };
        }

        // On a protected branch → block.
        let action = if matches!(input.target, GuardTarget::Commit { .. }) {
            "커밋할 수 없습니다"
        } else {
            "파일을 수정하려 합니다"
        };
        let reason = [
            format!("[Branch Guard] 보호 브랜치({current_branch})에서 {action}."),
            "먼저 새 브랜치를 생성해주세요:".to_string(),
            format!("  {} <branch-name>", input.create_branch_script),
        ]
        .join("\n");

        GuardOutput {
            allowed: false,
            reason: Some(reason),
            current_branch: Some(current_branch),
            default_branch: Some(default_branch),
        }
    }
}
