use std::path::Path;

use chrono::NaiveDate;
use tracing::info;

/// 보존 기간 초과 로그 파일 삭제
///
/// `daemon.YYYY-MM-DD.log` 형식의 파일 중 retention_days를 초과한 파일을 삭제.
/// 삭제 건수를 반환한다.
pub fn cleanup_old_logs(log_dir: &Path, retention_days: u32) -> u32 {
    let today = chrono::Local::now().date_naive();
    cleanup_old_logs_with_today(log_dir, retention_days, today)
}

/// 테스트 가능한 내부 구현: today를 주입받는다
fn cleanup_old_logs_with_today(log_dir: &Path, retention_days: u32, today: NaiveDate) -> u32 {
    let entries = match std::fs::read_dir(log_dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    let cutoff = today - chrono::Duration::days(retention_days as i64);
    let mut deleted = 0u32;

    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // daemon.YYYY-MM-DD.log 형식만 대상
        if let Some(date_str) = parse_log_date(&name_str) {
            if let Ok(file_date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                if file_date < cutoff && std::fs::remove_file(entry.path()).is_ok() {
                    info!("deleted old log: {name_str}");
                    deleted += 1;
                }
            }
        }
    }

    deleted
}

/// "daemon.YYYY-MM-DD.log" → Some("YYYY-MM-DD")
fn parse_log_date(filename: &str) -> Option<&str> {
    let rest = filename.strip_prefix("daemon.")?;
    let date_part = rest.strip_suffix(".log")?;
    // YYYY-MM-DD = 10 chars
    if date_part.len() == 10 {
        Some(date_part)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_log_file(dir: &Path, date: &str) {
        let name = format!("daemon.{date}.log");
        fs::write(dir.join(name), "test log content").unwrap();
    }

    #[test]
    fn parse_log_date_valid() {
        assert_eq!(parse_log_date("daemon.2026-02-20.log"), Some("2026-02-20"));
    }

    #[test]
    fn parse_log_date_invalid_prefix() {
        assert_eq!(parse_log_date("app.2026-02-20.log"), None);
    }

    #[test]
    fn parse_log_date_no_suffix() {
        assert_eq!(parse_log_date("daemon.2026-02-20"), None);
    }

    #[test]
    fn parse_log_date_wrong_length() {
        assert_eq!(parse_log_date("daemon.20260220.log"), None);
    }

    #[test]
    fn cleanup_deletes_old_files() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        // today = 2026-02-23, retention = 7 days → cutoff = 2026-02-16
        let today = NaiveDate::from_ymd_opt(2026, 2, 23).unwrap();

        make_log_file(dir, "2026-02-10"); // old → delete
        make_log_file(dir, "2026-02-15"); // old → delete
        make_log_file(dir, "2026-02-16"); // cutoff day → keep (not < cutoff)
        make_log_file(dir, "2026-02-22"); // recent → keep
        make_log_file(dir, "2026-02-23"); // today → keep

        let deleted = cleanup_old_logs_with_today(dir, 7, today);
        assert_eq!(deleted, 2);

        // 남은 파일 확인
        assert!(!dir.join("daemon.2026-02-10.log").exists());
        assert!(!dir.join("daemon.2026-02-15.log").exists());
        assert!(dir.join("daemon.2026-02-16.log").exists());
        assert!(dir.join("daemon.2026-02-22.log").exists());
        assert!(dir.join("daemon.2026-02-23.log").exists());
    }

    #[test]
    fn cleanup_ignores_non_log_files() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let today = NaiveDate::from_ymd_opt(2026, 2, 23).unwrap();

        make_log_file(dir, "2026-01-01"); // old → delete
        fs::write(dir.join("config.yaml"), "keep me").unwrap();
        fs::write(dir.join("daemon.pid"), "12345").unwrap();

        let deleted = cleanup_old_logs_with_today(dir, 7, today);
        assert_eq!(deleted, 1);

        assert!(dir.join("config.yaml").exists());
        assert!(dir.join("daemon.pid").exists());
    }

    #[test]
    fn cleanup_empty_dir_returns_zero() {
        let tmp = TempDir::new().unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 2, 23).unwrap();

        let deleted = cleanup_old_logs_with_today(tmp.path(), 30, today);
        assert_eq!(deleted, 0);
    }

    #[test]
    fn cleanup_nonexistent_dir_returns_zero() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 23).unwrap();
        let deleted = cleanup_old_logs_with_today(Path::new("/nonexistent/dir"), 30, today);
        assert_eq!(deleted, 0);
    }

    #[test]
    fn cleanup_zero_retention_deletes_all_except_today() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        let today = NaiveDate::from_ymd_opt(2026, 2, 23).unwrap();

        make_log_file(dir, "2026-02-22"); // yesterday → delete (< today)
        make_log_file(dir, "2026-02-23"); // today → keep

        let deleted = cleanup_old_logs_with_today(dir, 0, today);
        assert_eq!(deleted, 1);

        assert!(!dir.join("daemon.2026-02-22.log").exists());
        assert!(dir.join("daemon.2026-02-23.log").exists());
    }
}
