#![allow(dead_code)]

use anyhow::Result;
use autopilot::fs::FsOps;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A mock FsOps backed by an in-memory file map.
pub struct MockFs {
    files: HashMap<PathBuf, String>,
    pub written: Arc<Mutex<Vec<(PathBuf, String)>>>,
    pub removed: Arc<Mutex<Vec<PathBuf>>>,
}

impl Clone for MockFs {
    fn clone(&self) -> Self {
        Self {
            files: self.files.clone(),
            written: Arc::clone(&self.written),
            removed: Arc::clone(&self.removed),
        }
    }
}

impl MockFs {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
            written: Arc::new(Mutex::new(Vec::new())),
            removed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn with_file(mut self, path: &str, content: &str) -> Self {
        self.files.insert(PathBuf::from(path), content.to_string());
        self
    }

    /// Return all files written during the test.
    pub fn written_files(&self) -> Vec<(PathBuf, String)> {
        self.written.lock().unwrap().clone()
    }

    /// Return all files removed during the test.
    pub fn removed_files(&self) -> Vec<PathBuf> {
        self.removed.lock().unwrap().clone()
    }
}

impl FsOps for MockFs {
    fn read_file(&self, path: &Path) -> Result<String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("file not found: {}", path.display()))
    }

    fn write_file(&self, path: &Path, content: &str) -> Result<()> {
        self.written
            .lock()
            .unwrap()
            .push((path.to_path_buf(), content.to_string()));
        Ok(())
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    fn list_files(&self, dir: &Path, extension: &str) -> Result<Vec<PathBuf>> {
        let mut files: Vec<PathBuf> = self
            .files
            .keys()
            .filter(|p| p.parent() == Some(dir) && p.extension().map_or(false, |e| e == extension))
            .cloned()
            .collect();
        files.sort();
        Ok(files)
    }

    fn remove_file(&self, path: &Path) -> Result<()> {
        self.removed.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}
