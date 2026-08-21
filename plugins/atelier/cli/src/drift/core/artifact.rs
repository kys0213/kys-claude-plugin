//! The drift subsystem's outside edges: artifact file access and the backup
//! timestamp. Both are traits (DIP) so the check/sync rules run entirely in
//! memory under test — and so refusal-path tests can prove a rejected sync
//! wrote nothing at all. The real implementations sit alongside the traits,
//! like `session/core/baseline.rs`.

use crate::drift::core::types::ArtifactContent;
use crate::shared::shell::exec;

/// Every read, write and existence probe the drift commands perform. One trait
/// rather than per-role traits because check and sync use the same three
/// operations on the same files — splitting would only multiply doubles.
///
/// `read` classifies the decode outcome instead of failing on non-UTF-8 bytes:
/// an `Err` is reserved for IO failure, so the commands can judge (check) or
/// refuse (sync) an undecodable *user* file without conflating it with a
/// missing or unreadable one.
pub trait ArtifactFs {
    fn exists(&self, path: &str) -> bool;
    fn read(&self, path: &str) -> Result<ArtifactContent, String>;
    fn write(&self, path: &str, content: &str) -> Result<(), String>;
}

/// Real filesystem implementation.
pub struct RealArtifactFs;

pub fn create_artifact_fs() -> RealArtifactFs {
    RealArtifactFs
}

impl ArtifactFs for RealArtifactFs {
    /// `is_file`, matching the shell scripts' `[ -f ]` probes.
    fn exists(&self, path: &str) -> bool {
        std::path::Path::new(path).is_file()
    }

    fn read(&self, path: &str) -> Result<ArtifactContent, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
        Ok(match String::from_utf8(bytes) {
            Ok(content) => ArtifactContent::Utf8(content),
            Err(_) => ArtifactContent::NonUtf8,
        })
    }

    /// Content overwrite (`std::fs::write`), not write-then-rename: the target
    /// is the user's CLAUDE.md, whose inode and permissions must survive.
    fn write(&self, path: &str, content: &str) -> Result<(), String> {
        std::fs::write(path, content).map_err(|e| format!("{path}: {e}"))
    }
}

/// Source of the `<file>.bak-<timestamp>` suffix. Injectable so sync tests
/// are deterministic; the real clock answers in local time like the shell
/// scripts' `date` did.
pub trait BackupClock {
    /// Timestamp formatted `YYYYmmdd-HHMMSS`.
    fn backup_timestamp(&self) -> String;
}

/// Real clock. Local-time formatting is delegated to `date` because std
/// exposes no timezone database; the fallback keeps backups unique-per-second
/// even where `date` is unavailable.
pub struct LocalClock;

pub fn create_backup_clock() -> LocalClock {
    LocalClock
}

impl BackupClock for LocalClock {
    fn backup_timestamp(&self) -> String {
        let result = exec(&["date", "+%Y%m%d-%H%M%S"], None);
        if result.exit_code == 0 && !result.stdout.is_empty() {
            return result.stdout;
        }
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string())
    }
}
