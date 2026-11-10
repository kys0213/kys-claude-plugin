//! Starts a background Claude Code session (`claude --bg`) at a repository's
//! main working tree, optionally in a new git worktree.

use crate::kanban::core::request::{agent_header, check_slug};
use std::path::Path;
use std::process::Command;

pub struct SpawnArgs<'a> {
    pub cwd: &'a str,
    pub text: &'a str,
    pub worktree: Option<&'a str>,
    pub from_label: Option<&'a str>,
}

pub struct Spawned {
    pub short_id: String,
    pub cwd: String,
    pub worktree: Option<String>,
}

fn git(cwd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Accepts only the top of a repository's main working tree: a linked
/// worktree's git dir is `.git/worktrees/<name>`, the main one's is `.git`.
fn check_root(cwd: &str) -> Result<(), String> {
    let refuse = || {
        Err(format!(
            "`{cwd}` 는 git 저장소의 메인 작업 트리 루트가 아니에요"
        ))
    };
    if !Path::new(cwd).is_absolute() {
        return refuse();
    }
    let (Some(top), Some(git_dir)) = (
        git(cwd, &["rev-parse", "--show-toplevel"]),
        git(cwd, &["rev-parse", "--absolute-git-dir"]),
    ) else {
        return refuse();
    };
    let same = |a: &str, b: &Path| std::fs::canonicalize(a).ok() == std::fs::canonicalize(b).ok();
    if same(&top, Path::new(cwd)) && same(&git_dir, &Path::new(&top).join(".git")) {
        Ok(())
    } else {
        refuse()
    }
}

fn short_id(stdout: &str) -> Option<String> {
    let rest = stdout.split("backgrounded").nth(1)?;
    rest.trim_start_matches(|c: char| c.is_whitespace() || c == '·')
        .split(|c: char| !c.is_ascii_alphanumeric())
        .next()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

pub fn spawn(args: &SpawnArgs) -> Result<Spawned, String> {
    check_root(args.cwd)?;
    if let Some(name) = args.worktree {
        check_slug(name)?;
    }
    let body = args.text.trim();
    if body.is_empty() {
        return Err("맡길 일이 비어 있어요".to_string());
    }
    let prompt = format!("{}\n\n{body}", agent_header(args.from_label));
    let mut command = Command::new("claude");
    command.current_dir(args.cwd).arg("--bg");
    if let Some(name) = args.worktree {
        command.args(["--worktree", name]);
    }
    let out = command
        .arg(prompt)
        .output()
        .map_err(|e| format!("claude 를 실행하지 못했어요: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    match short_id(&stdout) {
        Some(short_id) if out.status.success() => Ok(Spawned {
            short_id,
            cwd: args.cwd.to_string(),
            worktree: args.worktree.map(str::to_string),
        }),
        _ => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let said = [stdout.trim(), stderr.trim()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" / ");
            Err(format!("새 세션을 띄우지 못했어요: {said}"))
        }
    }
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
