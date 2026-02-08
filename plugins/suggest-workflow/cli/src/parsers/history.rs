use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use anyhow::{Context, Result};
use crate::types::HistoryEntry;

/// Parse Claude history.jsonl file
#[allow(dead_code)]
pub fn parse_history_file(path: Option<&str>) -> Result<Vec<HistoryEntry>> {
    let file_path = if let Some(p) = path {
        PathBuf::from(p)
    } else {
        let home = std::env::var("HOME")
            .context("HOME environment variable not set")?;
        PathBuf::from(home).join(".claude").join("history.jsonl")
    };

    if !file_path.exists() {
        anyhow::bail!("History file not found: {}", file_path.display());
    }

    let file = File::open(&file_path)
        .with_context(|| format!("Failed to open history file: {}", file_path.display()))?;

    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;

        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<HistoryEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                eprintln!("Warning: Skipping line {} due to parse error: {}", line_num + 1, e);
            }
        }
    }

    Ok(entries)
}

/// Filter history entries by project path
#[allow(dead_code)]
pub fn filter_by_project(entries: &[HistoryEntry], project: &str) -> Vec<HistoryEntry> {
    entries.iter()
        .filter(|e| e.project == project)
        .cloned()
        .collect()
}

/// Filter by date range (timestamps in milliseconds)
#[allow(dead_code)]
pub fn filter_by_date_range(
    entries: &[HistoryEntry],
    start: i64,
    end: i64,
) -> Vec<HistoryEntry> {
    entries.iter()
        .filter(|e| e.timestamp >= start && e.timestamp <= end)
        .cloned()
        .collect()
}

/// Get unique project paths
#[allow(dead_code)]
pub fn get_unique_projects(entries: &[HistoryEntry]) -> Vec<String> {
    let mut projects: Vec<_> = entries.iter()
        .map(|e| e.project.clone())
        .collect();
    projects.sort();
    projects.dedup();
    projects
}
