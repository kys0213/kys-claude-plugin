//! Where a background session may start and what it is started with.

use super::request::{check_slug, prompt_text, Sender};
use std::path::PathBuf;

/// A directory and the repository around it, every path resolved.
pub struct RepoPaths {
    pub cwd: PathBuf,
    pub top: PathBuf,
    pub git_dir: PathBuf,
}

/// A linked worktree's git dir is `.git/worktrees/<name>`, the main one's is
/// `.git`, so only the main working tree's top passes.
pub fn is_main_root(paths: &RepoPaths) -> bool {
    paths.cwd == paths.top && paths.git_dir == paths.top.join(".git")
}

pub fn launch_args(
    text: &str,
    worktree: Option<&str>,
    from_label: Option<&str>,
) -> Result<Vec<String>, String> {
    if let Some(name) = worktree {
        check_slug(name)?;
    }
    let body = text.trim();
    if body.is_empty() {
        return Err("맡길 일이 비어 있어요".to_string());
    }
    let mut args = vec!["--bg".to_string()];
    if let Some(name) = worktree {
        args.extend(["--worktree".to_string(), name.to_string()]);
    }
    args.push(prompt_text(body, Sender::Agent, from_label));
    Ok(args)
}

/// The id `claude --bg` prints after `backgrounded ·`.
pub fn short_id(stdout: &str) -> Option<String> {
    let rest = stdout.split("backgrounded").nth(1)?;
    rest.trim_start_matches(|c: char| c.is_whitespace() || c == '·')
        .split(|c: char| !c.is_ascii_alphanumeric())
        .next()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_short_id_follows_the_backgrounded_mark() {
        assert_eq!(
            short_id("backgrounded · 6d652fd9\n  claude agents"),
            Some("6d652fd9".to_string())
        );
        assert_eq!(short_id("nothing here"), None);
    }
}
