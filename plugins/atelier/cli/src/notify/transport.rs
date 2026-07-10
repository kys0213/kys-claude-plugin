//! Delivery abstractions. The traits keep the command unit-testable; the real
//! HTTP implementation shells out to `curl` so the binary carries no HTTP
//! client dependency, and the file sink is a plain append. Failures surface
//! as per-channel errors in the report — never a non-zero exit.

use crate::git::core::shell::{exec, ExecResult};

/// JSON POST the notify command depends on (injectable for tests).
pub trait HttpPoster {
    fn post_json(&self, url: &str, body: &str) -> Result<(), String>;
}

/// Line append the `file` channel depends on (injectable for tests). One
/// event = one line, so pollers (`tail -F` under a Claude Code Monitor) see
/// exactly one event per line.
pub trait FileAppender {
    fn append_line(&self, path: &str, line: &str) -> Result<(), String>;
}

/// OS notification banner the `desktop` channel depends on (injectable for
/// tests).
pub trait DesktopNotifier {
    fn notify(&self, title: &str, body: &str) -> Result<(), String>;
}

/// Escapes a string for embedding in an AppleScript double-quoted literal.
pub fn applescript_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub struct RealDesktopNotifier;

impl DesktopNotifier for RealDesktopNotifier {
    #[cfg(target_os = "macos")]
    fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        let script = format!(
            "display notification \"{}\" with title \"{}\"",
            applescript_quote(body),
            applescript_quote(title)
        );
        run_notifier(&["osascript", "-e", &script])
    }

    #[cfg(not(target_os = "macos"))]
    fn notify(&self, title: &str, body: &str) -> Result<(), String> {
        run_notifier(&["notify-send", title, body])
    }
}

fn run_notifier(command: &[&str]) -> Result<(), String> {
    let result = exec(command, None);
    if result.exit_code != 0 {
        return Err(format!(
            "{} exit {}: {}",
            command[0], result.exit_code, result.stderr
        ));
    }
    Ok(())
}

pub struct RealFileAppender;

impl FileAppender for RealFileAppender {
    fn append_line(&self, path: &str, line: &str) -> Result<(), String> {
        use std::io::Write as _;
        if let Some(parent) = std::path::Path::new(path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(format!("{line}\n").as_bytes())
            .map_err(|e| e.to_string())
    }
}

/// Seconds curl may spend per delivery — a hook must not stall the session.
const CURL_MAX_TIME_SECS: &str = "5";

pub struct CurlPoster;

impl HttpPoster for CurlPoster {
    fn post_json(&self, url: &str, body: &str) -> Result<(), String> {
        let result: ExecResult = exec(
            &[
                "curl",
                "-sS",
                "--fail",
                "--max-time",
                CURL_MAX_TIME_SECS,
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "--data",
                body,
                url,
            ],
            None,
        );
        if result.exit_code != 0 {
            return Err(format!("curl exit {}: {}", result.exit_code, result.stderr));
        }
        Ok(())
    }
}
