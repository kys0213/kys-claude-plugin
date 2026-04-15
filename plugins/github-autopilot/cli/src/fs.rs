use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Abstraction over filesystem operations for testability.
pub trait FsOps: Send + Sync {
    /// Read the entire contents of a file as a string.
    fn read_file(&self, path: &Path) -> Result<String>;

    /// Write content to a file, creating parent directories as needed.
    fn write_file(&self, path: &Path, content: &str) -> Result<()>;

    /// Check whether a path exists.
    fn file_exists(&self, path: &Path) -> bool;

    /// List files in a directory (non-recursive) that match the given extension.
    fn list_files(&self, dir: &Path, extension: &str) -> Result<Vec<PathBuf>>;

    /// Remove a file. Returns Ok(()) even if the file does not exist.
    fn remove_file(&self, path: &Path) -> Result<()>;
}

/// Real implementation using `std::fs`.
pub struct RealFs;

impl FsOps for RealFs {
    fn read_file(&self, path: &Path) -> Result<String> {
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
    }

    fn write_file(&self, path: &Path, content: &str) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dir {}", parent.display()))?;
        }
        std::fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn list_files(&self, dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("failed to read dir {}", dir.display()))?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == extension {
                        files.push(path);
                    }
                }
            }
        }
        files.sort();
        Ok(files)
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(anyhow::anyhow!("failed to remove {}: {e}", path.display())),
        }
    }
}

/// Convenience: create a boxed real client.
pub fn real() -> Box<dyn FsOps> {
    Box::new(RealFs)
}
