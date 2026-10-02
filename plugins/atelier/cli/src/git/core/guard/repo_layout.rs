use std::path::{Component, Path, PathBuf};

/// Lexically collapses `.`/`..` in `path` (relative to `base`) without touching
/// the filesystem. A relative `path` is anchored at `base` rather than the
/// process cwd — the guard runs as a PreToolUse hook whose cwd may differ from
/// the project (worktree / subagent contexts), so resolving against cwd
/// mis-judges relative `file_path`s (#780).
pub(super) fn resolve_against(base: &Path, path: &str) -> PathBuf {
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
pub(super) fn resolve_project_dir(project_dir: &str) -> PathBuf {
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
    is_under(project, &resolve_against(project, file_path))
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
    find_repo_root(&resolved.parent().map(PathBuf::from).unwrap_or(resolved)).is_some()
}

/// Nearest ancestor of `path` (itself included) that holds a `.git` entry — a
/// directory for a main checkout, a file for a linked worktree. A `path` that
/// does not exist yet is first lifted to its nearest existing ancestor.
pub fn find_repo_root(path: &Path) -> Option<PathBuf> {
    let fs_root = Path::new("/");
    let mut dir = path.to_path_buf();

    while dir != fs_root && !dir.exists() {
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => break,
        }
    }
    while dir != fs_root {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => break,
        }
    }
    None
}

pub(super) fn is_under(base: &Path, path: &Path) -> bool {
    path.strip_prefix(base)
        .is_ok_and(|rel| !rel.starts_with(".."))
}

/// The git common directory of the repository checked out at `root`, read from
/// plain files: `.git/` itself for a main checkout, the `commondir` (or the
/// `<common>/worktrees/<name>` layout) behind a linked worktree's `.git` file.
fn common_git_dir(root: &Path) -> Option<PathBuf> {
    let canonical = |p: PathBuf| std::fs::canonicalize(&p).unwrap_or(p);
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(canonical(dot_git));
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let gitdir = text
        .lines()
        .find_map(|line| line.strip_prefix("gitdir:"))?
        .trim();
    let gitdir = resolve_against(root, gitdir);
    let common = match std::fs::read_to_string(gitdir.join("commondir")) {
        Ok(rel) => resolve_against(&gitdir, rel.trim()),
        Err(_) => {
            let worktrees = gitdir.parent()?;
            if worktrees.file_name()? != "worktrees" {
                return None;
            }
            worktrees.parent()?.to_path_buf()
        }
    };
    Some(canonical(common))
}

/// Whether `root` and `project_root` are checkouts of one repository.
pub(super) fn same_repository(root: &Path, project_root: Option<&Path>) -> bool {
    let Some(project_root) = project_root else {
        return false;
    };
    match (common_git_dir(root), common_git_dir(project_root)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
