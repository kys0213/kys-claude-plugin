use crate::shared::shell::exec;
use std::path::{Path, PathBuf};

pub trait ChangedFiles {
    /// Paths that exist at `head` and differ from the merge base of `base`
    /// and `head`.
    fn changed(&self, base: &str, head: &str) -> Result<Vec<String>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFile {
    pub path: String,
    pub content: String,
}

pub trait RuleFiles {
    /// Every `*.md` under `dir`, subdirectories included.
    fn list(&self, dir: &str) -> Result<Vec<RuleFile>, String>;
}

/// Runs git in the process cwd.
pub struct GitChangedFiles;

impl ChangedFiles for GitChangedFiles {
    fn changed(&self, base: &str, head: &str) -> Result<Vec<String>, String> {
        let range = format!("{base}...{head}");
        let result = exec(
            &[
                "git",
                "diff",
                "--name-only",
                "-z",
                "--diff-filter=d",
                &range,
            ],
            None,
        );
        if result.exit_code != 0 {
            return Err(format!("git diff {range} failed: {}", result.stderr));
        }
        Ok(result
            .stdout
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect())
    }
}

pub struct FsRuleFiles;

impl RuleFiles for FsRuleFiles {
    fn list(&self, dir: &str) -> Result<Vec<RuleFile>, String> {
        let root = Path::new(dir);
        if !root.is_dir() {
            return Err(format!("rules dir not found: {dir}"));
        }
        let mut paths = Vec::new();
        collect_md(root, &mut paths)?;
        paths.sort();
        paths
            .into_iter()
            .map(|path| {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                Ok(RuleFile {
                    path: path.to_string_lossy().to_string(),
                    content,
                })
            })
            .collect()
    }
}

/// Follows symlinks: user-level rule dirs are often dotfile links.
fn collect_md(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            collect_md(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
    Ok(())
}
