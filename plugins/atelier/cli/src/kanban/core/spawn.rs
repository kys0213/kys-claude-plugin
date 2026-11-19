//! Where a background session may start and what it is started with.

use super::request::{check_slug, prompt_text, Sender};
use super::terminal::TerminalEnv;
use serde_json::Value;
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

/// What runs the new session: Claude Code's own background, or a herdr-core
/// (`hc`) terminal started from the caller's terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Bg,
    Hc,
}

impl Via {
    /// `auto` is `hc` when the caller runs in an hc terminal of its own, as
    /// `env` (the caller's environment) says, else `bg`.
    pub fn parse(id: &str, env: &TerminalEnv) -> Result<Via, String> {
        match id {
            "bg" => Ok(Via::Bg),
            "hc" => Ok(Via::Hc),
            "auto" if env.hc_terminal().is_some() => Ok(Via::Hc),
            "auto" => Ok(Via::Bg),
            other => Err(format!("--via 는 bg, hc 또는 auto 예요 (받은 값: {other})")),
        }
    }

    /// The key of the started session's handle in `spawned`, and the handle
    /// read from what the launch printed.
    pub fn started(self, stdout: &str) -> Option<(&'static str, String)> {
        match self {
            Via::Bg => short_id(stdout).map(|id| ("short_id", id)),
            Via::Hc => terminal_address(stdout).map(|address| ("terminal", address)),
        }
    }
}

/// A program and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: &'static str,
    pub args: Vec<String>,
}

pub struct Task<'a> {
    pub text: &'a str,
    pub worktree: Option<&'a str>,
    pub from_label: Option<&'a str>,
}

pub fn launch(via: Via, cwd: &str, task: &Task) -> Result<Launch, String> {
    if let Some(name) = task.worktree {
        check_slug(name)?;
    }
    let body = task.text.trim();
    if body.is_empty() {
        return Err("맡길 일이 비어 있어요".to_string());
    }
    let mut claude = Vec::new();
    if let Some(name) = task.worktree {
        claude.extend(["--worktree".to_string(), name.to_string()]);
    }
    claude.push(prompt_text(body, Sender::Agent, task.from_label));
    Ok(match via {
        Via::Bg => Launch {
            program: "claude",
            args: [vec!["--bg".to_string()], claude].concat(),
        },
        Via::Hc => Launch {
            program: "hc",
            args: [
                [
                    "run",
                    "--bg",
                    "--json",
                    "--outlive-parent",
                    "--no-notify",
                    "--cwd",
                    cwd,
                    "--",
                    "claude",
                ]
                .map(str::to_string)
                .to_vec(),
                claude,
            ]
            .concat(),
        },
    })
}

/// The id `claude --bg` prints after `backgrounded ·`.
fn short_id(stdout: &str) -> Option<String> {
    let rest = stdout.split("backgrounded").nth(1)?;
    rest.trim_start_matches(|c: char| c.is_whitespace() || c == '·')
        .split(|c: char| !c.is_ascii_alphanumeric())
        .next()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

/// The `address` of the terminal `hc run --json` describes.
fn terminal_address(stdout: &str) -> Option<String> {
    let value: Value = serde_json::from_str(stdout.trim()).ok()?;
    value["address"]
        .as_str()
        .filter(|address| !address.is_empty())
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
