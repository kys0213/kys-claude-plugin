use anyhow::{Context, Result};
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::db::repository::*;
use crate::parsers;
use crate::analyzers::tool_classifier;
use crate::types::{Content, SessionEntry, ToolUse};

pub fn run(repo: &dyn IndexRepository, sessions_dir: &Path) -> Result<()> {
    repo.initialize()?;

    let session_files = parsers::list_sessions(sessions_dir)?;

    let mut new_count: u64 = 0;
    let mut updated_count: u64 = 0;
    let mut unchanged_count: u64 = 0;
    let mut error_count: u64 = 0;

    for file_path in &session_files {
        let meta = match std::fs::metadata(file_path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Warning: Cannot read {}: {}", file_path.display(), e);
                error_count += 1;
                continue;
            }
        };

        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let status = repo.check_session(file_path, size, mtime)?;

        if status == SessionStatus::Unchanged {
            unchanged_count += 1;
            continue;
        }

        match extract_session_data(file_path, size, mtime) {
            Ok(session_data) => {
                repo.upsert_session(&session_data)?;
                match status {
                    SessionStatus::New => new_count += 1,
                    SessionStatus::Changed => updated_count += 1,
                    SessionStatus::Unchanged => unreachable!(),
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to parse {}: {}",
                    file_path.display(),
                    e
                );
                error_count += 1;
            }
        }
    }

    // Remove sessions whose files no longer exist
    let existing_paths: Vec<&Path> = session_files.iter().map(|p| p.as_path()).collect();
    let deleted_count = repo.remove_stale_sessions(&existing_paths)?;

    // Rebuild derived tables
    repo.rebuild_derived_tables()?;

    // Update meta
    repo.update_meta("last_indexed_at", &chrono::Utc::now().to_rfc3339())?;

    // Summary to stderr
    eprintln!(
        "Indexed: {} new, {} updated, {} unchanged, {} deleted",
        new_count, updated_count, unchanged_count, deleted_count
    );
    if error_count > 0 {
        eprintln!("Warnings: {} sessions skipped due to errors", error_count);
    }

    Ok(())
}

fn extract_session_data(file_path: &Path, size: u64, mtime: i64) -> Result<SessionData> {
    let entries = parsers::parse_session(file_path)?;
    let tools = parsers::extract_tool_sequence(&entries);

    let session_id = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("invalid session filename")?
        .to_string();

    let prompts = extract_prompts(&entries);
    let tool_uses = classify_tool_uses(&tools);
    let file_edits = extract_file_edits(&tools);

    let first_ts = prompts
        .first()
        .map(|p| p.timestamp)
        .or_else(|| tool_uses.first().and_then(|t| t.timestamp));
    let last_ts = prompts
        .last()
        .map(|p| p.timestamp)
        .or_else(|| tool_uses.last().and_then(|t| t.timestamp));

    let first_prompt_snippet = prompts
        .first()
        .map(|p| p.text.chars().take(500).collect::<String>());

    Ok(SessionData {
        id: session_id,
        file_path: file_path.to_string_lossy().to_string(),
        file_size: size,
        file_mtime: mtime,
        first_ts,
        last_ts,
        prompt_count: prompts.len(),
        tool_use_count: tool_uses.len(),
        first_prompt_snippet,
        prompts,
        tool_uses,
        file_edits,
    })
}

fn extract_prompts(entries: &[SessionEntry]) -> Vec<PromptData> {
    entries
        .iter()
        .filter(|e| e.entry_type == "user")
        .filter_map(|e| {
            let message = e.message.as_ref()?;
            let text = match &message.content {
                Content::Text(t) => t.clone(),
                Content::Array(items) => items
                    .iter()
                    .filter(|i| i.item_type == "text")
                    .filter_map(|i| i.text.clone())
                    .collect::<Vec<_>>()
                    .join("\n"),
            };

            if text.trim().is_empty() {
                return None;
            }

            let timestamp = e
                .timestamp
                .as_ref()
                .and_then(|ts| chrono::DateTime::parse_from_rfc3339(ts).ok())
                .map(|dt| dt.timestamp_millis())
                .unwrap_or(0);

            Some(PromptData {
                char_count: text.chars().count(),
                text,
                timestamp,
            })
        })
        .collect()
}

fn classify_tool_uses(tools: &[ToolUse]) -> Vec<ToolUseData> {
    tools
        .iter()
        .enumerate()
        .map(|(i, tool)| {
            let classified = tool_classifier::classify_tool(&tool.name, tool.input.as_ref());
            ToolUseData {
                seq_order: i,
                tool_name: tool.name.clone(),
                classified_name: classified.classified_name,
                timestamp: tool.timestamp,
                input_json: tool.input.as_ref().map(|v| v.to_string()),
            }
        })
        .collect()
}

fn extract_file_edits(tools: &[ToolUse]) -> Vec<FileEditData> {
    tools
        .iter()
        .enumerate()
        .filter_map(|(i, tool)| {
            let file_path = match tool.name.as_str() {
                "Edit" | "Write" => tool
                    .input
                    .as_ref()
                    .and_then(|v| v.get("file_path"))
                    .and_then(|v| v.as_str()),
                "NotebookEdit" => tool
                    .input
                    .as_ref()
                    .and_then(|v| v.get("notebook_path"))
                    .and_then(|v| v.as_str()),
                _ => None,
            };

            file_path.map(|path| FileEditData {
                tool_use_seq: i,
                file_path: path.to_string(),
                timestamp: tool.timestamp,
            })
        })
        .collect()
}
