use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::tui::views::{LogLevel, LogLine};

/// Tracks file position for tail-like reading of daemon log files.
pub struct LogTailer {
    log_dir: PathBuf,
    current_date: String,
    file_offset: u64,
}

impl LogTailer {
    pub fn new(log_dir: PathBuf) -> Self {
        Self {
            log_dir,
            current_date: String::new(),
            file_offset: 0,
        }
    }

    /// Read new lines from the current day's daemon log file.
    /// Returns newly appended lines since the last read.
    pub fn poll_new_lines(&mut self) -> Vec<LogLine> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();

        // Date changed — reset offset to read from beginning of new file
        if today != self.current_date {
            self.current_date = today.clone();
            self.file_offset = 0;
        }

        let log_path = self.log_file_path(&today);
        if !log_path.exists() {
            return Vec::new();
        }

        self.read_new_lines(&log_path)
    }

    /// Initial load: read the last N lines from the log file.
    pub fn initial_load(&mut self, max_lines: usize) -> Vec<LogLine> {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.current_date = today.clone();

        let log_path = self.log_file_path(&today);
        if !log_path.exists() {
            return Vec::new();
        }

        match File::open(&log_path) {
            Ok(file) => {
                let reader = BufReader::new(&file);
                let all_lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
                let total = all_lines.len();
                let start = total.saturating_sub(max_lines);
                let lines: Vec<LogLine> = all_lines[start..]
                    .iter()
                    .map(|l| parse_log_line(l))
                    .collect();

                // Set offset to end of file for future incremental reads
                if let Ok(metadata) = file.metadata() {
                    self.file_offset = metadata.len();
                }

                lines
            }
            Err(_) => Vec::new(),
        }
    }

    fn log_file_path(&self, date: &str) -> PathBuf {
        self.log_dir.join(format!("daemon.{date}.log"))
    }

    fn read_new_lines(&mut self, path: &Path) -> Vec<LogLine> {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };

        let metadata = match file.metadata() {
            Ok(m) => m,
            Err(_) => return Vec::new(),
        };

        let file_len = metadata.len();
        if file_len <= self.file_offset {
            // File hasn't grown (or was truncated)
            if file_len < self.file_offset {
                self.file_offset = 0; // File was truncated, re-read from start
            }
            return Vec::new();
        }

        let mut reader = BufReader::new(file);
        if reader.seek(SeekFrom::Start(self.file_offset)).is_err() {
            return Vec::new();
        }

        let mut new_lines = Vec::new();
        let mut line_buf = String::new();
        loop {
            line_buf.clear();
            match reader.read_line(&mut line_buf) {
                Ok(0) => break,
                Ok(n) => {
                    self.file_offset += n as u64;
                    let trimmed = line_buf.trim_end().to_string();
                    if !trimmed.is_empty() {
                        new_lines.push(parse_log_line(&trimmed));
                    }
                }
                Err(_) => break,
            }
        }

        new_lines
    }
}

/// Parse a log line to extract the log level for color coding.
pub fn parse_log_line(line: &str) -> LogLine {
    let level = detect_log_level(line);
    LogLine {
        raw: line.to_string(),
        level,
    }
}

fn detect_log_level(line: &str) -> LogLevel {
    // Match common log formats:
    // tracing: "2026-02-22T14:32:00 INFO autodev::daemon ..."
    // or: "[INFO]", "ERROR", "WARN", etc.
    let upper = line.to_uppercase();
    if upper.contains(" ERROR ") || upper.contains("[ERROR]") || upper.contains("ERROR:") {
        LogLevel::Error
    } else if upper.contains(" WARN ") || upper.contains("[WARN]") || upper.contains("WARN:") {
        LogLevel::Warn
    } else if upper.contains(" INFO ") || upper.contains("[INFO]") || upper.contains("INFO:") {
        LogLevel::Info
    } else if upper.contains(" DEBUG ") || upper.contains("[DEBUG]") || upper.contains("DEBUG:") {
        LogLevel::Debug
    } else if upper.contains(" TRACE ") || upper.contains("[TRACE]") || upper.contains("TRACE:") {
        LogLevel::Trace
    } else {
        LogLevel::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_parse_log_line_levels() {
        assert_eq!(
            parse_log_line("2026-02-22T14:32:00 INFO starting daemon").level,
            LogLevel::Info
        );
        assert_eq!(
            parse_log_line("2026-02-22T14:32:00 ERROR scan failed: timeout").level,
            LogLevel::Error
        );
        assert_eq!(
            parse_log_line("2026-02-22T14:32:00 WARN retrying item").level,
            LogLevel::Warn
        );
        assert_eq!(
            parse_log_line("2026-02-22T14:32:00 DEBUG processing queue").level,
            LogLevel::Debug
        );
        assert_eq!(
            parse_log_line("some random line").level,
            LogLevel::Unknown
        );
    }

    #[test]
    fn test_log_tailer_initial_load() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().to_path_buf();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let log_path = log_dir.join(format!("daemon.{today}.log"));

        // Write some log lines
        {
            let mut f = File::create(&log_path).unwrap();
            for i in 0..100 {
                writeln!(f, "2026-02-22T14:00:{i:02} INFO line {i}").unwrap();
            }
        }

        let mut tailer = LogTailer::new(log_dir);
        let lines = tailer.initial_load(10);
        assert_eq!(lines.len(), 10);
        assert!(lines[0].raw.contains("line 90"));
        assert!(lines[9].raw.contains("line 99"));
    }

    #[test]
    fn test_log_tailer_incremental_poll() {
        let tmp = TempDir::new().unwrap();
        let log_dir = tmp.path().to_path_buf();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let log_path = log_dir.join(format!("daemon.{today}.log"));

        // Write initial lines
        {
            let mut f = File::create(&log_path).unwrap();
            writeln!(f, "2026-02-22T14:00:00 INFO initial line").unwrap();
        }

        let mut tailer = LogTailer::new(log_dir);
        let lines = tailer.initial_load(50);
        assert_eq!(lines.len(), 1);

        // Append new lines
        {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&log_path)
                .unwrap();
            writeln!(f, "2026-02-22T14:01:00 WARN new warning").unwrap();
            writeln!(f, "2026-02-22T14:02:00 ERROR something broke").unwrap();
        }

        let new_lines = tailer.poll_new_lines();
        assert_eq!(new_lines.len(), 2);
        assert_eq!(new_lines[0].level, LogLevel::Warn);
        assert_eq!(new_lines[1].level, LogLevel::Error);
    }

    #[test]
    fn test_log_tailer_no_file() {
        let tmp = TempDir::new().unwrap();
        let mut tailer = LogTailer::new(tmp.path().to_path_buf());
        let lines = tailer.initial_load(50);
        assert!(lines.is_empty());
        let new = tailer.poll_new_lines();
        assert!(new.is_empty());
    }
}
