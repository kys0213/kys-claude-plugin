#![allow(dead_code)]

use anyhow::Result;
use autopilot::fs::FsOps;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Shared mutable state for MockFs, enabling stateful multi-step tests.
#[derive(Clone, Default)]
struct Inner {
    files: HashMap<PathBuf, String>,
    written: Vec<(PathBuf, String)>,
    removed: Vec<PathBuf>,
}

/// A mock FsOps backed by an in-memory file map.
/// Writes and removes mutate the internal file map so that subsequent
/// reads, list_files, and file_exists calls reflect the changes.
#[derive(Clone)]
pub struct MockFs {
    inner: Arc<Mutex<Inner>>,
}

impl Default for MockFs {
    fn default() -> Self {
        Self::new()
    }
}

impl MockFs {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
        }
    }

    pub fn with_file(self, path: &str, content: &str) -> Self {
        self.inner
            .lock()
            .unwrap()
            .files
            .insert(PathBuf::from(path), content.to_string());
        self
    }

    /// Return all files written during the test.
    pub fn written_files(&self) -> Vec<(PathBuf, String)> {
        self.inner.lock().unwrap().written.clone()
    }

    /// Return all files removed during the test.
    pub fn removed_files(&self) -> Vec<PathBuf> {
        self.inner.lock().unwrap().removed.clone()
    }
}

impl FsOps for MockFs {
    fn read_file(&self, path: &Path) -> Result<String> {
        self.inner
            .lock()
            .unwrap()
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("file not found: {}", path.display()))
    }

    fn write_file(&self, path: &Path, content: &str) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        let path = path.to_path_buf();
        let content = content.to_string();
        inner.files.insert(path.clone(), content.clone());
        inner.written.push((path, content));
        Ok(())
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.inner.lock().unwrap().files.contains_key(path)
    }

    fn list_files(&self, dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
        let inner = self.inner.lock().unwrap();
        let mut files: Vec<PathBuf> = inner
            .files
            .keys()
            .filter(|p| p.parent() == Some(dir) && p.extension().is_some_and(|e| e == extension))
            .cloned()
            .collect();
        files.sort();
        Ok(files)
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.files.remove(path);
        inner.removed.push(path.to_path_buf());
        Ok(())
    }
}
