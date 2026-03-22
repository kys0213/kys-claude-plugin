use std::fs::{File, OpenOptions};
use std::path::Path;

use anyhow::{bail, Result};

// PID file format: `<pid>:<start_time_secs>\n`
// The start_time prevents false-positive "already running" when the OS
// recycles PIDs after a daemon crash.

const PID_FILE: &str = "daemon.pid";
const LOCK_FILE: &str = "daemon.pid.lock";

/// Maximum seconds to wait for the daemon process to exit after SIGTERM.
const STOP_TIMEOUT_SECS: u64 = 10;
/// Polling interval while waiting for process exit.
const STOP_POLL_MS: u64 = 200;

// ─── Public API ───

/// Acquire an advisory lock, check for stale PID files, then write the
/// current process's PID + start-time atomically.
///
/// Returns `Err` if another live daemon already owns the PID file.
pub fn write_pid(home: &Path) -> Result<()> {
    let _lock = lock(home)?;

    // Re-check under lock to close the TOCTOU window.
    if let Some((pid, recorded_start)) = read_pid_record(home) {
        if is_same_process(pid, recorded_start) {
            bail!("daemon is already running (pid: {pid})");
        }
        // Stale PID file — previous daemon crashed / OS recycled the PID.
        tracing::warn!("removing stale PID file (pid: {pid})");
        let _ = std::fs::remove_file(home.join(PID_FILE));
    }

    let pid = std::process::id();
    let start_time = get_process_start_time(pid).unwrap_or(0);
    let content = format!("{pid}:{start_time}\n");

    // Atomic write: write to tmp then rename.
    let tmp = home.join("daemon.pid.tmp");
    std::fs::write(&tmp, &content)?;
    std::fs::rename(&tmp, home.join(PID_FILE))?;

    Ok(())
}

/// Read the PID from the PID file (returns only the PID for callers that
/// don't care about start-time).
pub fn read_pid(home: &Path) -> Option<u32> {
    read_pid_record(home).map(|(pid, _)| pid)
}

/// Returns `true` if a daemon process is currently running.
///
/// This performs PID-recycling–safe validation by comparing the recorded
/// start-time against the live process.
pub fn is_running(home: &Path) -> bool {
    if let Some((pid, recorded_start)) = read_pid_record(home) {
        is_same_process(pid, recorded_start)
    } else {
        false
    }
}

/// Remove the PID file.
pub fn remove_pid(home: &Path) {
    let _ = std::fs::remove_file(home.join(PID_FILE));
}

/// Send SIGTERM and wait (poll) until the process exits, then remove the
/// PID file.  Avoids premature removal that could let a new daemon start
/// before the old one finishes its graceful shutdown.
pub fn stop(home: &Path) -> Result<()> {
    let pid = read_pid(home).ok_or_else(|| anyhow::anyhow!("daemon is not running"))?;

    // Validate that the recorded process is actually our daemon.
    if let Some((_, recorded_start)) = read_pid_record(home) {
        if !is_same_process(pid, recorded_start) {
            // PID file is stale — clean it up.
            remove_pid(home);
            bail!("daemon is not running (stale PID file removed)");
        }
    }

    send_sigterm(pid)?;

    // Poll until the process exits or timeout.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(STOP_TIMEOUT_SECS);
    loop {
        if !process_exists(pid) {
            break;
        }
        if std::time::Instant::now() >= deadline {
            tracing::warn!(
                "daemon (pid: {pid}) did not exit within {STOP_TIMEOUT_SECS}s — \
                 removing PID file anyway"
            );
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(STOP_POLL_MS));
    }

    remove_pid(home);
    println!("autodev daemon stopped (pid: {pid})");
    Ok(())
}

// ─── Internal helpers ───

/// Parse the PID file and return `(pid, start_time_secs)`.
fn read_pid_record(home: &Path) -> Option<(u32, u64)> {
    let content = std::fs::read_to_string(home.join(PID_FILE)).ok()?;
    let trimmed = content.trim();

    // Support legacy format (pid only, no colon).
    if let Some((pid_str, start_str)) = trimmed.split_once(':') {
        let pid = pid_str.parse().ok()?;
        let start = start_str.parse().unwrap_or(0);
        Some((pid, start))
    } else {
        // Legacy format: just a PID number.
        let pid = trimmed.parse().ok()?;
        Some((pid, 0))
    }
}

/// Check if `pid` is alive **and** its start-time matches `recorded_start`.
///
/// When `recorded_start == 0` (legacy file or unsupported platform) we fall
/// back to `process_exists` alone — same behaviour as before.
fn is_same_process(pid: u32, recorded_start: u64) -> bool {
    if !process_exists(pid) {
        return false;
    }
    if recorded_start == 0 {
        // Legacy PID file — cannot verify start time; assume alive is good enough.
        return true;
    }
    match get_process_start_time(pid) {
        Some(actual_start) => actual_start == recorded_start,
        None => false, // cannot read start time → treat as stale
    }
}

/// Cross-platform process existence check (signal 0).
fn process_exists(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: kill(pid, 0) is a standard POSIX existence check.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Send SIGTERM to a process.
fn send_sigterm(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        // SAFETY: SIGTERM is a standard, non-destructive signal.
        let ret = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        if ret != 0 {
            bail!(
                "failed to send SIGTERM to pid {pid}: {}",
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        bail!("SIGTERM is not supported on this platform");
    }
}

/// Get the process start time (seconds since epoch) via platform-specific
/// mechanisms.
///
/// - **macOS**: `sysctl kern.proc.pid.<pid>`
/// - **Linux**: `/proc/<pid>/stat` field 22 (start-time in clock ticks) +
///   `/proc/stat` btime
#[cfg(target_os = "macos")]
fn get_process_start_time(pid: u32) -> Option<u64> {
    // Use `ps` to get the process start time as seconds-since-epoch.
    // `ps -o lstart= -p <pid>` outputs a human-readable date; instead we
    // use `ps -o start= -p <pid>` and parse, but the most reliable
    // cross-version approach on macOS is to read via `proc_pidinfo`.
    // For simplicity and portability we shell out to `ps`.
    let output = std::process::Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let lstart = String::from_utf8_lossy(&output.stdout);
    let lstart = lstart.trim();
    if lstart.is_empty() {
        return None;
    }
    // Parse "Day Mon DD HH:MM:SS YYYY" format.
    chrono::NaiveDateTime::parse_from_str(lstart, "%a %b %e %H:%M:%S %Y")
        .ok()
        .map(|dt| dt.and_utc().timestamp() as u64)
}

#[cfg(target_os = "linux")]
fn get_process_start_time(pid: u32) -> Option<u64> {
    // /proc/<pid>/stat field 22 is starttime in clock ticks since boot.
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;

    // The comm field (field 2) may contain spaces/parens, so find the last ')'.
    let after_comm = stat.rfind(')')? + 2; // skip ") "
    let fields: Vec<&str> = stat[after_comm..].split_whitespace().collect();
    // Field 22 is at index 19 (fields start at field 3 after comm).
    let starttime_ticks: u64 = fields.get(19)?.parse().ok()?;

    // Read boot time from /proc/stat.
    let proc_stat = std::fs::read_to_string("/proc/stat").ok()?;
    let btime_line = proc_stat.lines().find(|l| l.starts_with("btime "))?;
    let btime: u64 = btime_line.split_whitespace().nth(1)?.parse().ok()?;

    let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as u64;
    if ticks_per_sec == 0 {
        return None;
    }

    Some(btime + starttime_ticks / ticks_per_sec)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn get_process_start_time(_pid: u32) -> Option<u64> {
    None
}

/// Acquire an advisory file lock (blocking).  Returns the `File` handle —
/// the lock is held as long as this handle is alive.
fn lock(home: &Path) -> Result<File> {
    let lock_path = home.join(LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)?;

    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if ret != 0 {
            bail!(
                "failed to acquire PID lock: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    Ok(file)
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn write_and_read_pid_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        write_pid(dir.path()).unwrap();

        let pid = read_pid(dir.path()).unwrap();
        assert_eq!(pid, std::process::id());
    }

    #[test]
    fn is_running_returns_true_for_self() {
        let dir = tempfile::tempdir().unwrap();
        write_pid(dir.path()).unwrap();
        assert!(is_running(dir.path()));
    }

    #[test]
    fn is_running_returns_false_when_no_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_running(dir.path()));
    }

    #[test]
    fn stale_pid_detected_wrong_start_time() {
        let dir = tempfile::tempdir().unwrap();
        // Write a PID file for our own PID but with a wrong start-time.
        let pid = std::process::id();
        fs::write(dir.path().join(PID_FILE), format!("{pid}:99999999\n")).unwrap();

        // Should detect stale — start time mismatch.
        assert!(!is_running(dir.path()));
    }

    #[test]
    fn stale_pid_detected_nonexistent_process() {
        let dir = tempfile::tempdir().unwrap();
        // Use PID 1_999_999 which almost certainly doesn't exist.
        fs::write(dir.path().join(PID_FILE), "1999999:0\n").unwrap();

        assert!(!is_running(dir.path()));
    }

    #[test]
    fn legacy_pid_file_format_supported() {
        let dir = tempfile::tempdir().unwrap();
        // Legacy format: just PID, no colon.
        let pid = std::process::id();
        fs::write(dir.path().join(PID_FILE), format!("{pid}\n")).unwrap();

        // Should still work — falls back to process_exists only.
        assert!(is_running(dir.path()));
        assert_eq!(read_pid(dir.path()), Some(pid));
    }

    #[test]
    fn write_pid_rejects_live_daemon() {
        let dir = tempfile::tempdir().unwrap();
        write_pid(dir.path()).unwrap();

        // Second write should fail (same process is "running").
        let err = write_pid(dir.path()).unwrap_err();
        assert!(err.to_string().contains("already running"));
    }

    #[test]
    fn write_pid_cleans_stale_and_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        // Plant a stale PID file.
        fs::write(dir.path().join(PID_FILE), "1999999:12345\n").unwrap();

        // Should succeed after cleaning stale file.
        write_pid(dir.path()).unwrap();
        assert_eq!(read_pid(dir.path()), Some(std::process::id()));
    }

    #[test]
    fn remove_pid_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        remove_pid(dir.path()); // no file — should not panic
        write_pid(dir.path()).unwrap();
        remove_pid(dir.path());
        assert!(!dir.path().join(PID_FILE).exists());
        remove_pid(dir.path()); // already removed — should not panic
    }

    #[test]
    fn read_pid_record_parses_both_formats() {
        let dir = tempfile::tempdir().unwrap();

        // New format
        fs::write(dir.path().join(PID_FILE), "42:1234567890\n").unwrap();
        assert_eq!(read_pid_record(dir.path()), Some((42, 1234567890)));

        // Legacy format
        fs::write(dir.path().join(PID_FILE), "42\n").unwrap();
        assert_eq!(read_pid_record(dir.path()), Some((42, 0)));
    }
}
