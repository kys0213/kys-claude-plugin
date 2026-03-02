use std::path::Path;

use serde::{Deserialize, Serialize};

// ─── Status file models ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub updated_at: String,
    pub uptime_secs: u64,
    pub active_items: Vec<StatusItem>,
    pub counters: StatusCounters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusItem {
    pub work_id: String,
    pub queue_type: String,
    pub repo_name: String,
    pub number: i64,
    pub title: String,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatusCounters {
    pub wip: i64,
    pub done: i64,
    pub skip: i64,
    pub failed: i64,
}

// ─── Build / Write / Read ───

/// 활성 아이템 목록과 카운터로 DaemonStatus를 생성한다.
pub fn build_status(
    active_items: Vec<StatusItem>,
    counters: &StatusCounters,
    start_time: std::time::Instant,
) -> DaemonStatus {
    let wip = active_items.len() as i64;

    DaemonStatus {
        updated_at: chrono::Local::now().to_rfc3339(),
        uptime_secs: start_time.elapsed().as_secs(),
        active_items,
        counters: StatusCounters {
            wip,
            done: counters.done,
            skip: counters.skip,
            failed: counters.failed,
        },
    }
}

/// Atomic write: tmp → rename
pub fn write_status(path: &Path, status: &DaemonStatus) {
    let json = match serde_json::to_string_pretty(status) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!("failed to serialize status: {e}");
            return;
        }
    };

    let tmp = path.with_extension("tmp");
    if let Err(e) = std::fs::write(&tmp, &json) {
        tracing::warn!("failed to write status tmp file: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        tracing::warn!("failed to rename status file: {e}");
    }
}

/// status file 읽기 (없거나 파싱 실패 시 None)
pub fn read_status(path: &Path) -> Option<DaemonStatus> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

/// 데몬 종료 시 status file 삭제
pub fn remove_status(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_read_status_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("daemon.status.json");

        let status = DaemonStatus {
            updated_at: "2026-02-23T14:00:00+09:00".to_string(),
            uptime_secs: 3600,
            active_items: vec![StatusItem {
                work_id: "issue:org/repo:42".to_string(),
                queue_type: "issue".to_string(),
                repo_name: "org/repo".to_string(),
                number: 42,
                title: "Fix bug".to_string(),
                phase: "Pending".to_string(),
            }],
            counters: StatusCounters {
                wip: 1,
                done: 10,
                skip: 2,
                failed: 0,
            },
        };

        write_status(&path, &status);
        let loaded = read_status(&path).expect("should read back");

        assert_eq!(loaded.active_items.len(), 1);
        assert_eq!(loaded.active_items[0].work_id, "issue:org/repo:42");
        assert_eq!(loaded.counters.wip, 1);
        assert_eq!(loaded.counters.done, 10);
    }

    #[test]
    fn read_status_missing_file_returns_none() {
        let result = read_status(Path::new("/tmp/nonexistent-status.json"));
        assert!(result.is_none());
    }

    #[test]
    fn read_status_invalid_json_returns_none() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("bad.json");
        std::fs::write(&path, "not json").unwrap();

        assert!(read_status(&path).is_none());
    }

    #[test]
    fn remove_status_cleans_up() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("daemon.status.json");
        std::fs::write(&path, "{}").unwrap();
        assert!(path.exists());

        remove_status(&path);
        assert!(!path.exists());
    }
}
