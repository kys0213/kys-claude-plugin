//! Command execution port — the only side effect the notify subsystem
//! performs. Channels are user-declared argv commands; the runner spawns them
//! **without a shell** (argv passed verbatim), so event data substituted into
//! argv elements or stdin can never be interpreted as shell syntax.

use std::io::Read as _;
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Spawns `argv` with optional stdin, enforcing a deadline (injectable for
/// tests). A hook must not stall the session, so the real runner kills the
/// child at the deadline and reports it as a per-channel error.
pub trait CommandRunner {
    fn run(&self, argv: &[String], stdin: Option<&str>, timeout_secs: u64) -> Result<(), String>;
}

pub struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(&self, argv: &[String], stdin: Option<&str>, timeout_secs: u64) -> Result<(), String> {
        let Some(program) = argv.first() else {
            return Err("empty exec".to_string());
        };

        let mut cmd = Command::new(program);
        cmd.args(&argv[1..])
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("{program}: {e}"))?;

        if let (Some(input), Some(mut pipe)) = (stdin, child.stdin.take()) {
            // Ignore write errors (child may exit early); closing the pipe on
            // drop is what matters so `cat`-style readers terminate.
            let _ = pipe.write_all(input.as_bytes());
        }

        // Drain stderr on a thread so a chatty child can't deadlock on a full
        // pipe while we wait for it.
        let stderr_handle = child.stderr.take().map(|mut pipe| {
            std::thread::spawn(move || {
                let mut buf = String::new();
                let _ = pipe.read_to_string(&mut buf);
                buf
            })
        });

        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let status = loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(status) => break status,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("{program}: timed out after {timeout_secs}s"));
                }
                None => std::thread::sleep(Duration::from_millis(25)),
            }
        };

        let stderr = stderr_handle
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        if !status.success() {
            let code = status.code().unwrap_or(-1);
            return Err(format!("{program} exit {code}: {}", stderr.trim_end()));
        }
        Ok(())
    }
}
