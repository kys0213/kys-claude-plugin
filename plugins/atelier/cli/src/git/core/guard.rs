//! Default-branch guard, ported from git-utils `core/guard.ts` (unifies the
//! two hook scripts). Blocks writes/commits on protected branches and falls
//! open in ambiguous situations (no repo, special git state, detection
//! failure).

use regex::Regex;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use crate::git::types::{GuardInput, GuardOutput, GuardTarget};

use super::git::GitService;

static GIT_COMMIT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bgit\b.*\bcommit\b").unwrap());

/// Lexically resolves `p` to an absolute, normalized path (collapsing `.` and
/// `..`), mirroring Node's `path.resolve`.
fn lexical_abs(p: &str) -> PathBuf {
    let raw = Path::new(p);
    let base = if raw.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
    };
    let mut out = base;
    for comp in raw.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// True when `file_path` is the project dir itself or nested inside it.
pub fn is_inside_project_dir(file_path: &str, project_dir: &str) -> bool {
    let proj = lexical_abs(project_dir);
    let file = lexical_abs(file_path);
    file.starts_with(&proj)
}

/// True when `file_path` lives inside some git repository — ascending from the
/// nearest existing ancestor and looking for a `.git` entry.
pub fn is_inside_any_git_repo(file_path: &str) -> bool {
    let resolved = lexical_abs(file_path);
    let mut dir = resolved.parent().map(Path::to_path_buf).unwrap_or(resolved);

    // Ascend to the first existing directory (cheaper than probing each level).
    while dir.parent().is_some() && !dir.exists() {
        dir = dir.parent().unwrap().to_path_buf();
    }

    loop {
        if dir.join(".git").exists() {
            return true;
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => return false,
        }
    }
}

/// The default-branch guard contract.
pub trait GuardService {
    fn check(&self, input: &GuardInput) -> GuardOutput;
}

/// Real guard backed by an injected [`GitService`].
pub struct RealGuardService<'a> {
    git: &'a dyn GitService,
}

impl<'a> RealGuardService<'a> {
    pub fn new(git: &'a dyn GitService) -> Self {
        Self { git }
    }
}

fn allow(reason: Option<&str>) -> GuardOutput {
    GuardOutput {
        allowed: true,
        reason: reason.map(str::to_string),
        current_branch: None,
        default_branch: None,
    }
}

impl GuardService for RealGuardService<'_> {
    fn check(&self, input: &GuardInput) -> GuardOutput {
        // write guard: a file outside the project but also outside any git
        // repo (e.g. a settings file) is allowed; inside another repo it falls
        // through to the normal guard.
        if input.target == GuardTarget::Write {
            if let Some(fp) = &input.tool_file_path {
                if !is_inside_project_dir(fp, &input.project_dir) && !is_inside_any_git_repo(fp) {
                    return allow(Some("file is outside any git repository"));
                }
            }
        }

        // commit guard: only `git commit` commands are guarded.
        if input.target == GuardTarget::Commit {
            let is_commit = input
                .tool_command
                .as_deref()
                .map(|c| GIT_COMMIT_PATTERN.is_match(c))
                .unwrap_or(false);
            if !is_commit {
                return allow(Some("not a git commit command"));
            }
        }

        // Guard 1: must be a git repo.
        if !matches!(self.git.is_inside_work_tree(), Ok(true)) {
            return allow(Some("not a git repository"));
        }

        // Resolve the default branch (explicit > detected; detection failure
        // falls open).
        let default_branch = match &input.default_branch {
            Some(b) => b.clone(),
            None => match self.git.detect_default_branch() {
                Ok(b) => b,
                Err(_) => return allow(Some("could not detect default branch")),
            },
        };

        // Guard 2/3: special state or detached HEAD falls open.
        let state = match self.git.get_special_state() {
            Ok(s) => s,
            Err(_) => return allow(None),
        };
        if state.rebase || state.merge {
            return allow(Some("special git state (rebase/merge)"));
        }
        if state.detached {
            return allow(Some("detached HEAD"));
        }

        // Protected set: default branch + develop + extras.
        let mut protected: HashSet<String> = HashSet::new();
        protected.insert(default_branch.clone());
        protected.insert("develop".to_string());
        if let Some(extras) = &input.protected_branches {
            for b in extras {
                protected.insert(b.clone());
            }
        }

        let current_branch = self.git.get_current_branch().unwrap_or_default();

        if !protected.contains(&current_branch) {
            return GuardOutput {
                allowed: true,
                reason: None,
                current_branch: Some(current_branch),
                default_branch: Some(default_branch),
            };
        }

        // Working on a protected branch → block.
        let action = if input.target == GuardTarget::Commit {
            "커밋할 수 없습니다"
        } else {
            "파일을 수정하려 합니다"
        };
        let reason = format!(
            "[Branch Guard] 보호 브랜치({current_branch})에서 {action}.\n\
             먼저 새 브랜치를 생성해주세요:\n  \
             {} <branch-name>",
            input.create_branch_script
        );
        GuardOutput {
            allowed: false,
            reason: Some(reason),
            current_branch: Some(current_branch),
            default_branch: Some(default_branch),
        }
    }
}
