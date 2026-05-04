//! Shared output helpers for CLI subcommands.

use std::io::Write;

use anyhow::Result;
use serde::Serialize;

/// Serialize `value` as a single line of compact JSON followed by a newline.
/// Used by every subcommand that supports `--json`.
pub fn write_json<T: Serialize>(out: &mut dyn Write, value: &T) -> Result<()> {
    serde_json::to_writer(&mut *out, value)?;
    writeln!(out)?;
    Ok(())
}
