//! The code-style subsystem's filesystem edge. It extends drift's
//! `ArtifactFs` instead of widening it, so drift keeps its own write semantics
//! (no parent creation) and its test doubles stay as they are, while the
//! verbatim-copy judgement drift already owns is reused unchanged.

use crate::drift::core::artifact::{ArtifactFs, RealArtifactFs};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirEntries {
    /// Regular files directly inside the directory, by name, sorted.
    pub files: Vec<String>,
    /// Subdirectories directly inside the directory, by name, sorted.
    pub dirs: Vec<String>,
}

pub trait RuleTreeFs: ArtifactFs {
    fn is_dir(&self, path: &str) -> bool;
    fn list_dir(&self, path: &str) -> Result<DirEntries, String>;
    /// Writes `content`, creating any missing parent directories first.
    fn write_creating_parents(&self, path: &str, content: &str) -> Result<(), String>;
    fn remove_file(&self, path: &str) -> Result<(), String>;
}

impl RuleTreeFs for RealArtifactFs {
    fn is_dir(&self, path: &str) -> bool {
        std::path::Path::new(path).is_dir()
    }

    fn list_dir(&self, path: &str) -> Result<DirEntries, String> {
        let mut entries = DirEntries::default();
        for entry in std::fs::read_dir(path).map_err(|e| format!("{path}: {e}"))? {
            let entry = entry.map_err(|e| format!("{path}: {e}"))?;
            let name = entry.file_name().to_string_lossy().to_string();
            // `metadata` follows symlinks, so a linked rule file counts as a file.
            match std::fs::metadata(entry.path()) {
                Ok(meta) if meta.is_file() => entries.files.push(name),
                Ok(meta) if meta.is_dir() => entries.dirs.push(name),
                _ => {}
            }
        }
        entries.files.sort();
        entries.dirs.sort();
        Ok(entries)
    }

    fn write_creating_parents(&self, path: &str, content: &str) -> Result<(), String> {
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        self.write(path, content)
    }

    fn remove_file(&self, path: &str) -> Result<(), String> {
        std::fs::remove_file(path).map_err(|e| format!("{path}: {e}"))
    }
}
