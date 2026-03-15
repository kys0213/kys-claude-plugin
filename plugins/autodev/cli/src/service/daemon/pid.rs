use std::path::Path;

use anyhow::Result;

pub fn write_pid(home: &Path) -> Result<()> {
    let pid = std::process::id();
    std::fs::write(home.join("daemon.pid"), pid.to_string())?;
    Ok(())
}

pub fn read_pid(home: &Path) -> Option<u32> {
    std::fs::read_to_string(home.join("daemon.pid"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

pub fn is_running(home: &Path) -> bool {
    if let Some(pid) = read_pid(home) {
        process_exists(pid)
    } else {
        false
    }
}

/// Cross-platform process existence check
fn process_exists(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        Path::new(&format!("/proc/{pid}")).exists()
    }
}

pub fn remove_pid(home: &Path) {
    let _ = std::fs::remove_file(home.join("daemon.pid"));
}
