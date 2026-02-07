use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use chrono::DateTime;
use crate::types::{SessionEntry, ToolUse, Content, HistoryEntry};

const DEFAULT_PROJECTS_PATH: &str = ".claude/projects";

/// List all projects in ~/.claude/projects
pub fn list_projects(base_path: Option<&str>) -> Result<Vec<String>> {
    let projects_path = if let Some(p) = base_path {
        PathBuf::from(p)
    } else {
        let home = std::env::var("HOME")?;
        PathBuf::from(home).join(DEFAULT_PROJECTS_PATH)
    };

    if !projects_path.exists() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();
    for entry in fs::read_dir(&projects_path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                projects.push(name.to_string());
            }
        }
    }

    projects.sort();
    Ok(projects)
}

/// List all session files for a project
pub fn list_sessions(project_path: &Path) -> Result<Vec<PathBuf>> {
    if !project_path.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(project_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            sessions.push(path);
        }
    }

    sessions.sort();
    Ok(sessions)
}

/// Parse a single session file
pub fn parse_session(session_path: &Path) -> Result<Vec<SessionEntry>> {
    let file = File::open(session_path)
        .with_context(|| format!("Failed to open session: {}", session_path.display()))?;

    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;

        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<SessionEntry>(&line) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                eprintln!("Warning: Skipping line {} in {}: {}",
                    line_num + 1, session_path.display(), e);
            }
        }
    }

    Ok(entries)
}

/// Extract user prompts from session entries
pub fn extract_user_prompts(entries: &[SessionEntry]) -> Vec<String> {
    entries.iter()
        .filter(|e| e.entry_type == "user")
        .filter_map(|e| e.message.as_ref())
        .filter_map(|m| match &m.content {
            Content::Text(text) => Some(text.clone()),
            Content::Array(items) => {
                let texts: Vec<String> = items.iter()
                    .filter(|item| item.item_type == "text")
                    .filter_map(|item| item.text.clone())
                    .collect();
                if texts.is_empty() {
                    None
                } else {
                    Some(texts.join("\n"))
                }
            }
        })
        .collect()
}

/// Extract tool usage sequence from session entries
pub fn extract_tool_sequence(entries: &[SessionEntry]) -> Vec<ToolUse> {
    let mut tools = Vec::new();

    for entry in entries {
        // Legacy format: top-level tool_use
        if entry.entry_type == "tool_use" {
            if let Some(name) = &entry.name {
                let timestamp = entry.timestamp.as_ref()
                    .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                    .map(|dt| dt.timestamp_millis());

                tools.push(ToolUse {
                    name: name.clone(),
                    timestamp,
                });
            }
            continue;
        }

        // Current format: tool_use nested in assistant messages
        if entry.entry_type == "assistant" {
            if let Some(message) = &entry.message {
                if let Content::Array(items) = &message.content {
                    for item in items {
                        if item.item_type == "tool_use" {
                            if let Some(name) = &item.name {
                                let timestamp = entry.timestamp.as_ref()
                                    .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                                    .map(|dt| dt.timestamp_millis());

                                tools.push(ToolUse {
                                    name: name.clone(),
                                    timestamp,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    tools
}

/// Resolve project path to ~/.claude/projects/encoded-name
pub fn resolve_project_path(raw_path: &str) -> Option<PathBuf> {
    let normalized = Path::new(raw_path)
        .canonicalize()
        .ok()?
        .to_string_lossy()
        .to_string()
        .trim_end_matches('/')
        .to_string();

    // Encode: replace "/" with "-" and prepend "-"
    let encoded = format!("-{}", normalized[1..].replace('/', "-"));

    let home = std::env::var("HOME").ok()?;
    let project_path = PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(encoded);

    if project_path.exists() {
        Some(project_path)
    } else {
        None
    }
}

/// Adapt session entries to history entries format
pub fn adapt_to_history_entries(
    sessions: &[(String, Vec<SessionEntry>)],
    project_path: &str,
) -> Vec<HistoryEntry> {
    let mut history_entries = Vec::new();

    for (_session_id, entries) in sessions {
        for entry in entries {
            if entry.entry_type == "user" {
                if let Some(message) = &entry.message {
                    let display = match &message.content {
                        Content::Text(text) => text.clone(),
                        Content::Array(items) => {
                            items.iter()
                                .filter(|item| item.item_type == "text")
                                .filter_map(|item| item.text.clone())
                                .collect::<Vec<_>>()
                                .join("\n")
                        }
                    };

                    // Strip system-reminder blocks first
                    let display = strip_system_reminders(&display);

                    if display.trim().is_empty() {
                        continue;
                    }

                    // Filter system meta messages
                    if is_system_meta_message(&display) {
                        continue;
                    }

                    let timestamp = entry.timestamp.as_ref()
                        .and_then(|ts| DateTime::parse_from_rfc3339(ts).ok())
                        .map(|dt| dt.timestamp_millis())
                        .unwrap_or(0);

                    history_entries.push(HistoryEntry {
                        display,
                        timestamp,
                        project: project_path.to_string(),
                    });
                }
            }
        }
    }

    history_entries
}

/// Strip <system-reminder>...</system-reminder> blocks from user messages
fn strip_system_reminders(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut rest = content;

    while let Some(start) = rest.find("<system-reminder>") {
        result.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("</system-reminder>") {
            rest = &rest[start + end + "</system-reminder>".len()..];
        } else {
            // Unclosed tag - skip everything after it
            rest = "";
            break;
        }
    }
    result.push_str(rest);
    result.trim().to_string()
}

fn is_system_meta_message(content: &str) -> bool {
    let trimmed = content.trim();
    let lower = trimmed.to_lowercase();

    // Basic meta filters
    if lower.starts_with("<local-command-") ||
       lower.starts_with("<command-name>") ||
       lower.contains("[request interrupted by user") ||
       trimmed.len() < 3 {
        return true;
    }

    // Skill/command expansion: starts with "# " and very long
    if trimmed.starts_with("# ") && trimmed.len() > 500 {
        return true;
    }

    // Mode activation prompts
    if lower.contains("[autopilot activated") ||
       lower.contains("[ralph loop") ||
       lower.contains("[ultrawork activated") ||
       lower.contains("[ralplan activated") ||
       lower.contains("[ecomode activated") {
        return true;
    }

    // Predominantly markdown table content (system docs)
    let line_count = trimmed.lines().count();
    if line_count > 5 {
        let table_lines = trimmed.lines()
            .filter(|l| l.trim().starts_with('|') && l.trim().ends_with('|'))
            .count();
        if table_lines as f64 / line_count as f64 > 0.5 {
            return true;
        }
    }

    // YAML frontmatter (command definitions)
    if trimmed.starts_with("---\n") && trimmed.contains("\n---\n") {
        return true;
    }

    false
}
