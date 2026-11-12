//! Paths as this machine's file system resolves them.

use crate::kanban::core::allowlist::RealPaths;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub struct FsPaths;

impl RealPaths for FsPaths {
    /// Resolves one component at a time, so a link is followed before a `..`
    /// after it, as the kernel does. The part that does not exist stays as
    /// named.
    fn real(&self, path: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                Component::Normal(name) => {
                    out.push(name);
                    if let Ok(real) = fs::canonicalize(&out) {
                        out = real;
                    }
                }
                other => out.push(other),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_followed_before_the_parent_after_it() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir_all(base.join("a/deep")).unwrap();
        fs::create_dir_all(base.join("repo")).unwrap();
        std::os::unix::fs::symlink(base.join("a/deep"), base.join("repo/link")).unwrap();

        assert_eq!(FsPaths.real(&base.join("repo/link/../x")), base.join("a/x"));
        assert_eq!(
            FsPaths.real(&base.join("repo/new/f")),
            base.join("repo/new/f")
        );
    }
}
