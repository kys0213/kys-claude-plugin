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
        // /proc/<pid> 존재 여부로 프로세스 생존 확인
        Path::new(&format!("/proc/{pid}")).exists()
    } else {
        false
    }
}

pub fn remove_pid(home: &Path) {
    let _ = std::fs::remove_file(home.join("daemon.pid"));
}
