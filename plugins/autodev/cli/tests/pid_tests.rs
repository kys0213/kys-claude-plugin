use autodev::daemon::pid;
use tempfile::TempDir;

// ═══════════════════════════════════════════════
// PID 파일 기본 동작
// ═══════════════════════════════════════════════

#[test]
fn write_and_read_pid_returns_current_process() {
    let tmpdir = TempDir::new().unwrap();
    pid::write_pid(tmpdir.path()).expect("write_pid should succeed");

    let read = pid::read_pid(tmpdir.path());
    assert_eq!(
        read,
        Some(std::process::id()),
        "read_pid should return current process ID"
    );
}

#[test]
fn read_pid_missing_file_returns_none() {
    let tmpdir = TempDir::new().unwrap();
    assert_eq!(
        pid::read_pid(tmpdir.path()),
        None,
        "read_pid should return None when no PID file exists"
    );
}

// ═══════════════════════════════════════════════
// is_running 검증
// ═══════════════════════════════════════════════

#[test]
fn is_running_returns_true_for_current_process() {
    let tmpdir = TempDir::new().unwrap();
    pid::write_pid(tmpdir.path()).unwrap();

    assert!(
        pid::is_running(tmpdir.path()),
        "current process should be detected as running"
    );
}

#[test]
fn is_running_returns_false_when_no_pid_file() {
    let tmpdir = TempDir::new().unwrap();
    assert!(
        !pid::is_running(tmpdir.path()),
        "should return false when no PID file"
    );
}

#[test]
fn is_running_returns_false_for_nonexistent_pid() {
    let tmpdir = TempDir::new().unwrap();
    // PID 999999999는 존재하지 않을 가능성이 높음
    std::fs::write(tmpdir.path().join("daemon.pid"), "999999999").unwrap();

    assert!(
        !pid::is_running(tmpdir.path()),
        "nonexistent PID should not be detected as running"
    );
}

// ═══════════════════════════════════════════════
// remove_pid 검증
// ═══════════════════════════════════════════════

#[test]
fn remove_pid_deletes_file() {
    let tmpdir = TempDir::new().unwrap();
    pid::write_pid(tmpdir.path()).unwrap();

    assert!(tmpdir.path().join("daemon.pid").exists());
    pid::remove_pid(tmpdir.path());
    assert!(!tmpdir.path().join("daemon.pid").exists());
}

#[test]
fn remove_pid_no_error_when_file_missing() {
    let tmpdir = TempDir::new().unwrap();
    // 파일이 없어도 에러 없이 동작
    pid::remove_pid(tmpdir.path());
    assert!(!tmpdir.path().join("daemon.pid").exists());
}

// ═══════════════════════════════════════════════
// PID 파일 내용 형식
// ═══════════════════════════════════════════════

#[test]
fn pid_file_contains_numeric_string() {
    let tmpdir = TempDir::new().unwrap();
    pid::write_pid(tmpdir.path()).unwrap();

    let content = std::fs::read_to_string(tmpdir.path().join("daemon.pid")).unwrap();
    let parsed: u32 = content.trim().parse().expect("PID file should contain a valid u32");
    assert_eq!(parsed, std::process::id());
}

#[test]
fn read_pid_handles_non_numeric_content() {
    let tmpdir = TempDir::new().unwrap();
    std::fs::write(tmpdir.path().join("daemon.pid"), "not-a-number\n").unwrap();
    assert_eq!(
        pid::read_pid(tmpdir.path()),
        None,
        "non-numeric PID file should return None"
    );
}

#[test]
fn read_pid_handles_empty_file() {
    let tmpdir = TempDir::new().unwrap();
    std::fs::write(tmpdir.path().join("daemon.pid"), "").unwrap();
    assert_eq!(
        pid::read_pid(tmpdir.path()),
        None,
        "empty PID file should return None"
    );
}
