//! Subprocess execution utility — port of `git-utils/src/core/shell.ts`.
//! `exec` never throws and returns trimEnd'd stdout/stderr plus the exit code;
//! `exec_or_throw` returns the trimmed stdout or an error string on non-zero
//! exit, embedding the same message format as the TS `execOrThrow`.

use std::collections::HashMap;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, Default)]
pub struct ExecOptions {
    pub cwd: Option<String>,
    /// Extra env vars merged on top of the inherited environment.
    pub env: Option<HashMap<String, String>>,
}

/// Trims trailing whitespace the same way JS `String.prototype.trimEnd` does.
fn trim_end(s: &str) -> String {
    s.trim_end().to_string()
}

/// Runs `command` and returns its result without ever propagating an error.
/// A spawn failure is surfaced as exit code `-1` with the OS error on stderr,
/// matching the "never throw" contract of the TS `exec`.
pub fn exec(command: &[&str], options: Option<&ExecOptions>) -> ExecResult {
    if command.is_empty() {
        return ExecResult {
            stdout: String::new(),
            stderr: "empty command".to_string(),
            exit_code: -1,
        };
    }

    let mut cmd = Command::new(command[0]);
    cmd.args(&command[1..]);

    if let Some(opts) = options {
        if let Some(cwd) = &opts.cwd {
            cmd.current_dir(cwd);
        }
        if let Some(env) = &opts.env {
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
    }

    match cmd.output() {
        Ok(out) => ExecResult {
            stdout: trim_end(&String::from_utf8_lossy(&out.stdout)),
            stderr: trim_end(&String::from_utf8_lossy(&out.stderr)),
            exit_code: out.status.code().unwrap_or(-1),
        },
        Err(e) => ExecResult {
            stdout: String::new(),
            stderr: e.to_string(),
            exit_code: -1,
        },
    }
}

/// Runs `command` and returns trimmed stdout, or an error string on non-zero
/// exit. The error message mirrors the TS format exactly.
pub fn exec_or_throw(command: &[&str], options: Option<&ExecOptions>) -> Result<String, String> {
    let result = exec(command, options);
    if result.exit_code != 0 {
        return Err(format!(
            "Command failed (exit {}): {}\n{}",
            result.exit_code,
            command.join(" "),
            result.stderr
        ));
    }
    Ok(result.stdout)
}
