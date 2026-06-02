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

/// Normalizes a path the way Node's `path.resolve` does for the comparison:
/// makes it absolute against cwd if relative, then lexically collapses
/// `.`/`..` without touching the filesystem.
fn resolve_lexical(p: &str) -> PathBuf {
    let path = Path::new(p);
    let mut base = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
    };
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                base.pop();
            }
            Component::CurDir => {}
            other => base.push(other.as_os_str()),
        }
    }
    base
}

/// Port of TS `isInsideProjectDir`: true when `file_path` is the project dir
/// itself or strictly inside it (no `..` escape, not a sibling prefix match).
pub fn is_inside_project_dir(file_path: &str, project_dir: &str) -> bool {
    let project = resolve_lexical(project_dir);
    let file = resolve_lexical(file_path);
    match file.strip_prefix(&project) {
        Ok(rel) => {
            // rel == '' (same dir) or a normal relative descendant.
            rel.as_os_str().is_empty() || !rel.starts_with("..")
        }
        Err(_) => false,
    }
}

/// Port of TS `isInsideAnyGitRepo`: walks up from the file's directory looking
/// for a `.git` entry, skipping non-existent leading directories first.
pub fn is_inside_any_git_repo(file_path: &str) -> bool {
    let resolved = resolve_lexical(file_path);
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

        // write guard: file outside the project directory.
        if input.target == GuardTarget::Write {
            if let Some(file_path) = &input.tool_file_path {
                if !is_inside_project_dir(file_path, &input.project_dir)
                    && !is_inside_any_git_repo(file_path)
                {
                    return pass(Some("file is outside any git repository"));
                }
            }
        }

        // commit guard: not a git commit command → pass.
        if input.target == GuardTarget::Commit {
            let is_commit = input
                .tool_command
                .as_ref()
                .map(|c| !c.is_empty() && GIT_COMMIT_PATTERN.is_match(c))
                .unwrap_or(false);
            if !is_commit {
                return pass(Some("not a git commit command"));
            }
        }

        // Guard 1: inside a git repo.
        if !self.git.is_inside_work_tree() {
            return pass(Some("not a git repository"));
        }

        // Resolve default branch.
        let default_branch = match &input.default_branch {
            Some(b) => b.clone(),
            None => match self.git.detect_default_branch() {
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

        // Guard 2: special state (rebase/merge) → pass.
        let state = self.git.get_special_state();
        if state.rebase || state.merge {
            return pass(Some("special git state (rebase/merge)"));
        }

        // Guard 3: detached HEAD → pass.
        if state.detached {
            return pass(Some("detached HEAD"));
        }

        let current_branch = self.git.get_current_branch();

        if !protected.contains(&current_branch) {
            return GuardOutput {
                allowed: true,
                reason: None,
                current_branch: Some(current_branch),
                default_branch: Some(default_branch),
            };
        }

        // On a protected branch → block.
        let action = if input.target == GuardTarget::Commit {
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
