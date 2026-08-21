//! Drift command layer. Commands take their dependencies through `DriftDeps`
//! and return domain values — rendering and exit codes are the CLI edge's job,
//! so every rule here is exercisable in memory.

pub mod check;
pub mod sync;

use crate::drift::core::artifact::{ArtifactFs, BackupClock};
use crate::drift::core::types::ArtifactContent;

/// Everything the drift commands need from the outside world. Injected as one
/// struct (never repeated fn params) so a new dependency changes one type, not
/// every signature.
pub struct DriftDeps<'a> {
    pub fs: &'a dyn ArtifactFs,
    /// Only sync stamps backups, but the clock lives on the shared deps so
    /// wiring stays uniform across commands.
    pub clock: &'a dyn BackupClock,
}

/// Reads a plugin source file, which is plugin-owned and must be UTF-8. A
/// source that fails to decode is an environment error (hard `Err`, exit 2),
/// unlike user-side artifacts, whose encoding is a judgement input.
fn read_source(deps: &DriftDeps, path: &str) -> Result<String, String> {
    match deps.fs.read(path)? {
        ArtifactContent::Utf8(content) => Ok(content),
        ArtifactContent::NonUtf8 => Err(format!("plugin source file is not valid UTF-8: {path}")),
    }
}
