//! Default-branch guard — port of `git-utils/src/core/guard.ts`. Decides
//! whether a write/commit on a protected branch is allowed. `GuardService`
//! takes a `GitService` by injection so it is unit-testable with a mock git.

use crate::git::core::git::GitService;
use crate::git::types::{GuardInput, GuardOutput, GuardTarget};
use std::path::{Component, Path, PathBuf};

/// Value-taking git global options that consume the following token, so the
/// `commit` subcommand detection can skip over `-C <path>`, `-c <k=v>`, etc.
const VALUE_TAKING_GLOBAL_OPTS: &[&str] = &[
    "-C",
    "-c",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--super-prefix",
    "--exec-path",
];

/// True when `token` looks like a leading shell env assignment (`FOO=bar`).
fn is_env_assignment(token: &str) -> bool {
    match token.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name.chars().enumerate().all(|(i, c)| {
                    if i == 0 {
                        c.is_ascii_alphabetic() || c == '_'
                    } else {
                        c.is_ascii_alphanumeric() || c == '_'
                    }
                })
        }
        None => false,
    }
}

/// Decides whether `command` actually *invokes* `git commit`, rather than
/// merely containing the substrings "git" and "commit" somewhere (e.g. inside
/// a quoted `--body` of `gh issue create`). The previous `\bgit\b.*\bcommit\b`
/// regex was a substring match and produced false positives that blocked
/// unrelated commands on protected branches (#754).
///
/// Splits on shell separators (`&&`, `||`, `;`, `|`, newline) and only matches
/// when a segment's actual command token is `git` and its subcommand is
/// `commit`. (Quoting is not parsed exhaustively — this is a guard heuristic.)
pub fn is_git_commit_command(command: &str) -> bool {
    let normalized = command
        .replace("&&", "\n")
        .replace("||", "\n")
        .replace([';', '|'], "\n");

    for segment in normalized.split('\n') {
        let tokens: Vec<&str> = segment.split_whitespace().collect();

        // Skip leading env assignments (e.g. `GIT_AUTHOR_NAME=x git commit`).
        let mut i = 0;
        while i < tokens.len() && is_env_assignment(tokens[i]) {
            i += 1;
        }
        let Some(&cmd) = tokens.get(i) else {
            continue;
        };

        // Command token must be `git` (or a path ending in `/git`).
        if cmd != "git" && !cmd.ends_with("/git") {
            continue;
        }

        // First non-option token after `git` is the subcommand. Value-taking
        // global options consume their value token too.
        let mut j = i + 1;
        while let Some(tok) = tokens.get(j) {
            if tok.starts_with('-') {
                j += if VALUE_TAKING_GLOBAL_OPTS.contains(tok) {
                    2
                } else {
                    1
                };
            } else {
                break;
            }
        }
        if tokens.get(j) == Some(&"commit") {
            return true;
        }
    }
    false
}

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

        // commit guard: not a git commit command → pass (substring false
        // positives are avoided by token-based detection — #754).
        if input.target == GuardTarget::Commit {
            let is_commit = input
                .tool_command
                .as_ref()
                .is_some_and(|c| is_git_commit_command(c));
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
            // 진단 정보: 어느 디렉토리/브랜치를 기준으로 판정했는지 노출 (worktree 디버깅 — #754)
            format!("  평가 디렉토리: {}", input.project_dir),
            format!("  감지된 브랜치: {current_branch} (기본 브랜치: {default_branch})"),
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
