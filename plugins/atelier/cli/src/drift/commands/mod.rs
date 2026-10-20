pub mod check;
pub mod sync;

use crate::drift::core::artifact::{ArtifactFs, BackupClock};
use crate::drift::core::types::ArtifactContent;

pub struct DriftDeps<'a> {
    pub fs: &'a dyn ArtifactFs,
    pub clock: &'a dyn BackupClock,
}

fn read_source(deps: &DriftDeps, path: &str) -> Result<String, String> {
    match deps.fs.read(path)? {
        ArtifactContent::Utf8(content) => Ok(content),
        ArtifactContent::NonUtf8 => Err(format!("plugin source file is not valid UTF-8: {path}")),
    }
}

fn scope_installed(deps: &DriftDeps, copies: &[String]) -> bool {
    copies.iter().any(|copy| deps.fs.exists(copy))
}
