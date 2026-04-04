use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::process::Command;
use std::thread;

/// Run `gh` with the given arguments and return stdout.
/// Returns an error if `gh` is not found or exits with non-zero.
pub fn run(args: &[&str]) -> Result<String> {
    let output = Command::new("gh")
        .args(args)
        .output()
        .context("gh CLI not found")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run `gh` and return stdout even on non-zero exit (for commands where
/// empty results are valid). Returns Err only on spawn failure.
pub fn run_lenient(args: &[&str]) -> Result<String> {
    let output = Command::new("gh")
        .args(args)
        .output()
        .context("gh CLI not found")?;

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run `gh` with lenient execution and parse the result as a JSON array.
pub fn list_json(args: &[&str]) -> Result<Vec<Value>> {
    let raw = run_lenient(args)?;
    serde_json::from_str(&raw).context("failed to parse gh output as JSON array")
}

/// Run multiple tasks concurrently, returning results in order.
pub fn run_parallel<T: Send + 'static>(
    tasks: Vec<Box<dyn FnOnce() -> Result<T> + Send>>,
) -> Vec<Result<T>> {
    let handles: Vec<_> = tasks.into_iter().map(thread::spawn).collect();
    handles
        .into_iter()
        .map(|h| h.join().expect("thread panicked"))
        .collect()
}
