//! Subprocess execution primitive, ported from git-utils `core/shell.ts`.
//!
//! `exec` never fails the process — it captures stdout/stderr and the exit
//! code so callers can branch on it. `exec_or_throw` is the strict variant
//! that returns an error on a non-zero exit. Both trim trailing whitespace
//! from captured output, matching the TypeScript `trimEnd()` behaviour.

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

/// Captured result of a subprocess run.
#[derive(Debug, Clone)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Runs `command` (argv form, `command[0]` is the program) in `cwd`. Thin
/// wrapper over [`exec_env`] with no extra environment.
pub fn exec(command: &[&str], cwd: Option<&Path>) -> Result<ExecResult> {
    exec_env(command, cwd, &[])
}

/// Like [`exec`] but layers `env` (key/value pairs) on top of the inherited
/// environment. Never returns an error for a non-zero exit — the exit code is
/// surfaced in [`ExecResult::exit_code`].
pub fn exec_env(command: &[&str], cwd: Option<&Path>, env: &[(&str, &str)]) -> Result<ExecResult> {
    let (program, args) = command
        .split_first()
        .ok_or_else(|| anyhow::anyhow!("exec: empty command"))?;
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("failed to spawn `{program}`: {e}"))?;
    Ok(ExecResult {
        stdout: String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_string(),
        stderr: String::from_utf8_lossy(&output.stderr)
            .trim_end()
            .to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Like [`exec`] but returns an error (with stderr) when the command exits
/// non-zero. On success, returns the trimmed stdout.
pub fn exec_or_throw(command: &[&str], cwd: Option<&Path>) -> Result<String> {
    let result = exec(command, cwd)?;
    if result.exit_code != 0 {
        bail!(
            "Command failed (exit {}): {}\n{}",
            result.exit_code,
            command.join(" "),
            result.stderr
        );
    }
    Ok(result.stdout)
}
