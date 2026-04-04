use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::process::Command;
use std::sync::Arc;
use std::thread;

/// Abstraction over GitHub CLI operations for testability.
pub trait GhOps: Send + Sync {
    /// Run `gh` and require success (non-zero exit = error).
    fn run(&self, args: &[&str]) -> Result<String>;
    /// Run `gh` and return stdout even on non-zero exit.
    fn list_json(&self, args: &[&str]) -> Result<Vec<Value>>;
}

/// Real implementation that shells out to `gh`.
pub struct RealGh;

impl GhOps for RealGh {
    fn run(&self, args: &[&str]) -> Result<String> {
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

    fn list_json(&self, args: &[&str]) -> Result<Vec<Value>> {
        let output = Command::new("gh")
            .args(args)
            .output()
            .context("gh CLI not found")?;

        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        serde_json::from_str(&raw).context("failed to parse gh output as JSON array")
    }
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

/// Count items from a gh list command.
pub fn count_items(gh: &dyn GhOps, args: &[&str]) -> Result<u64> {
    let items = gh.list_json(args)?;
    Ok(items.len() as u64)
}

/// Convenience: create a shared real client.
pub fn real() -> Arc<dyn GhOps> {
    Arc::new(RealGh)
}
