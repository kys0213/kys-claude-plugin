//! The process edges of `kanban spawn`: git for where the session would run,
//! the launch program (`claude` or `hc`) to start it.

use crate::kanban::core::spawn::{Launch, RepoPaths};
use std::path::{Path, PathBuf};
use std::process::Command;

pub trait RepoProbe {
    /// `None` when `cwd` is not an absolute path inside a git repository.
    fn paths(&self, cwd: &str) -> Option<RepoPaths>;
}

pub struct Launched {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait Launcher {
    fn launch(&self, cwd: &str, launch: &Launch) -> Result<Launched, String>;
}

pub struct GitProbe;

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

fn resolved(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

impl RepoProbe for GitProbe {
    fn paths(&self, cwd: &str) -> Option<RepoPaths> {
        if !Path::new(cwd).is_absolute() {
            return None;
        }
        Some(RepoPaths {
            cwd: resolved(Path::new(cwd))?,
            top: resolved(Path::new(&git(cwd, &["rev-parse", "--show-toplevel"])?))?,
            git_dir: resolved(Path::new(&git(cwd, &["rev-parse", "--absolute-git-dir"])?))?,
        })
    }
}

pub struct ProcessLauncher;

impl Launcher for ProcessLauncher {
    fn launch(&self, cwd: &str, launch: &Launch) -> Result<Launched, String> {
        let out = Command::new(launch.program)
            .current_dir(cwd)
            .args(&launch.args)
            .output()
            .map_err(|e| format!("{} 를 실행하지 못했어요: {e}", launch.program))?;
        Ok(Launched {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        })
    }
}
