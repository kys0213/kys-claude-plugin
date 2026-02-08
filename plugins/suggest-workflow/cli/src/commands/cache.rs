use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use crate::types::*;
use crate::parsers::{
    parse_session, list_sessions, resolve_project_path,
    adapt_to_history_entries, extract_tool_sequence,
};
use crate::analyzers::{
    analyze_workflows, analyze_prompts, analyze_tacit_knowledge,
    AnalysisDepth, DepthConfig,
};
use crate::analyzers::tool_classifier::classify_tool;

const CACHE_VERSION: &str = "1.0.0";
const CACHE_DIR_NAME: &str = "suggest-workflow-cache";

// Seed keywords for classifying prompt types in summaries
const DIRECTIVE_KEYWORDS: &[&str] = &[
    "항상", "반드시", "무조건", "절대", "꼭", "always", "must", "never",
];
const CORRECTION_KEYWORDS: &[&str] = &[
    "말고", "대신", "아니라", "아니야", "틀렸", "instead",
];

/// Generate cache files for a project.
/// Outputs the cache directory path to stdout.
pub fn run(
    project_path: &str,
    depth: &AnalysisDepth,
    threshold: usize,
    top: usize,
    decay: bool,
) -> Result<()> {
    let resolved_path = resolve_project_path(project_path)
        .with_context(|| format!(
            "Project not found: {}\nExpected encoded directory under ~/.claude/projects/",
            project_path
        ))?;

    let encoded_name = resolved_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let cache_dir = get_cache_dir(&encoded_name)?;
    let sessions_dir = cache_dir.join("sessions");
    fs::create_dir_all(&sessions_dir)?;

    eprintln!("Cache: {}", cache_dir.display());

    // Load existing cache index for incremental processing
    let existing_index = load_existing_index(&cache_dir);
    let existing_sizes: HashMap<String, u64> = existing_index
        .as_ref()
        .map(|idx| {
            idx.sessions
                .iter()
                .map(|s| (s.id.clone(), s.file_size))
                .collect()
        })
        .unwrap_or_default();

    let session_files = list_sessions(&resolved_path)?;
    if session_files.is_empty() {
        eprintln!("No sessions found.");
        return Ok(());
    }

    let mut session_metas: Vec<CacheSessionMeta> = Vec::new();
    let mut all_sessions: Vec<(String, Vec<SessionEntry>)> = Vec::new();
    let mut total_prompts = 0;
    let mut new_count = 0;
    let mut cached_count = 0;

    for session_file in &session_files {
        let session_id = session_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let file_size = fs::metadata(session_file)
            .map(|m| m.len())
            .unwrap_or(0);

        // Parse session (always needed for analysis snapshot)
        let entries = parse_session(session_file)?;

        // Check if summary is cached (JSONL is append-only → size match = unchanged)
        let summary_path = sessions_dir.join(format!("{}.summary.json", session_id));
        let is_cached = existing_sizes
            .get(&session_id)
            .map_or(false, |&cached_size| {
                cached_size == file_size && summary_path.exists()
            });

        let summary = if is_cached {
            cached_count += 1;
            load_summary(&summary_path)?
        } else {
            let s = generate_session_summary(&session_id, &entries, project_path);
            let json = serde_json::to_string_pretty(&s)?;
            fs::write(&summary_path, &json)?;
            new_count += 1;
            s
        };

        let meta = build_meta_from_summary(&session_id, session_file, file_size, &summary);
        total_prompts += meta.prompt_count;
        session_metas.push(meta);
        all_sessions.push((session_id, entries));
    }

    eprintln!(
        "Sessions: {} new, {} cached, {} total",
        new_count, cached_count, session_metas.len()
    );

    // Generate analysis snapshot
    let depth_config = depth.resolve();
    let history_entries = adapt_to_history_entries(&all_sessions, project_path);

    generate_analysis_snapshot(
        &all_sessions,
        &history_entries,
        &depth_config,
        depth,
        threshold,
        top,
        decay,
        project_path,
        &cache_dir,
    )?;

    // Write index
    let index = CacheIndex {
        project: project_path.to_string(),
        project_encoded: encoded_name,
        last_updated: Utc::now().to_rfc3339(),
        cache_version: CACHE_VERSION.to_string(),
        sessions: session_metas,
        total_prompts,
        total_sessions: session_files.len(),
    };

    let index_json = serde_json::to_string_pretty(&index)?;
    fs::write(cache_dir.join("index.json"), &index_json)?;

    eprintln!("Cache generated successfully.");
    // Output cache directory path for downstream consumers
    println!("{}", cache_dir.display());

    Ok(())
}

// --- Helpers ---

fn get_cache_dir(encoded_name: &str) -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home)
        .join(".claude")
        .join(CACHE_DIR_NAME)
        .join(encoded_name))
}

fn load_existing_index(cache_dir: &Path) -> Option<CacheIndex> {
    let path = cache_dir.join("index.json");
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn load_summary(path: &Path) -> Result<SessionSummary> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read summary: {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse summary: {}", path.display()))
}

// --- Session summary generation ---

fn generate_session_summary(
    session_id: &str,
    entries: &[SessionEntry],
    project_path: &str,
) -> SessionSummary {
    let sessions = vec![(session_id.to_string(), entries.to_vec())];
    let history_entries = adapt_to_history_entries(&sessions, project_path);

    // Prompts with type classification
    let prompts: Vec<SummaryPrompt> = history_entries
        .iter()
        .map(|e| SummaryPrompt {
            text: e.display.clone(),
            timestamp: e.timestamp,
            prompt_type: classify_prompt_type(&e.display),
        })
        .collect();

    // Tool extraction and classification
    let tool_uses = extract_tool_sequence(entries);
    let classified: Vec<String> = tool_uses
        .iter()
        .map(|t| classify_tool(&t.name, t.input.as_ref()).classified_name)
        .collect();

    let tool_use_count = tool_uses.len();
    let tool_sequences = build_sequence_strings(&tool_uses, &classified);

    // Directives and corrections (deduplicated)
    let directives: Vec<String> = prompts
        .iter()
        .filter(|p| p.prompt_type.as_deref() == Some("directive"))
        .map(|p| truncate_str(&p.text, 120))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let corrections: Vec<String> = prompts
        .iter()
        .filter(|p| p.prompt_type.as_deref() == Some("correction"))
        .map(|p| truncate_str(&p.text, 120))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    // Mutated files
    let files_mutated = extract_mutated_files(&tool_uses);

    // Static signals
    let unique_tools: HashSet<&str> = classified.iter().map(|s| s.as_str()).collect();
    let static_signals = StaticSignals {
        has_directive: !directives.is_empty(),
        has_correction: !corrections.is_empty(),
        prompt_density: match prompts.len() {
            0..=10 => "low",
            11..=30 => "medium",
            _ => "high",
        }
        .to_string(),
        workflow_complexity: match unique_tools.len() {
            0..=4 => "simple",
            5..=8 => "medium",
            _ => "complex",
        }
        .to_string(),
    };

    SessionSummary {
        id: session_id.to_string(),
        prompts,
        tool_use_count,
        tool_sequences,
        directives,
        corrections,
        files_mutated,
        static_signals,
    }
}

fn classify_prompt_type(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for keyword in DIRECTIVE_KEYWORDS {
        if lower.contains(keyword) {
            return Some("directive".to_string());
        }
    }
    for keyword in CORRECTION_KEYWORDS {
        if lower.contains(keyword) {
            return Some("correction".to_string());
        }
    }
    None
}

fn build_sequence_strings(tool_uses: &[ToolUse], classified: &[String]) -> Vec<String> {
    if classified.len() < 2 {
        return Vec::new();
    }

    let time_window = 5 * 60 * 1000_i64;
    let mut work_units: Vec<Vec<String>> = Vec::new();
    let mut current_unit: Vec<String> = Vec::new();

    for (i, name) in classified.iter().enumerate() {
        current_unit.push(name.clone());

        if i < tool_uses.len() - 1 {
            if let (Some(curr_ts), Some(next_ts)) =
                (tool_uses[i].timestamp, tool_uses[i + 1].timestamp)
            {
                if next_ts - curr_ts > time_window {
                    if current_unit.len() >= 2 {
                        work_units.push(current_unit.clone());
                    }
                    current_unit.clear();
                }
            }
        }
    }

    if current_unit.len() >= 2 {
        work_units.push(current_unit);
    }

    // Deduplicate and limit
    work_units
        .iter()
        .map(|unit| unit.join(" → "))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(30)
        .collect()
}

fn extract_mutated_files(tool_uses: &[ToolUse]) -> Vec<String> {
    let mut files: BTreeSet<String> = BTreeSet::new();
    for tool in tool_uses {
        if matches!(tool.name.as_str(), "Edit" | "Write" | "NotebookEdit") {
            if let Some(input) = &tool.input {
                // Edit/Write use "file_path", NotebookEdit uses "notebook_path"
                let path = input
                    .get("file_path")
                    .or_else(|| input.get("notebook_path"))
                    .and_then(|v| v.as_str());
                if let Some(p) = path {
                    files.insert(p.to_string());
                }
            }
        }
    }
    files.into_iter().collect()
}

// --- Meta builder ---

fn build_meta_from_summary(
    session_id: &str,
    session_file: &Path,
    file_size: u64,
    summary: &SessionSummary,
) -> CacheSessionMeta {
    let prompt_count = summary.prompts.len();

    // Timestamps
    let first_ts = summary.prompts.first().map(|p| p.timestamp);
    let last_ts = summary.prompts.last().map(|p| p.timestamp);

    let first_timestamp = first_ts
        .and_then(|ts| DateTime::from_timestamp_millis(ts).map(|dt| dt.to_rfc3339()));
    let last_timestamp = last_ts
        .and_then(|ts| DateTime::from_timestamp_millis(ts).map(|dt| dt.to_rfc3339()));

    let duration_minutes = match (first_ts, last_ts) {
        (Some(first), Some(last)) if last > first => Some((last - first) / (60 * 1000)),
        _ => None,
    };

    // Dominant tools from sequences
    let mut tool_counts: HashMap<String, usize> = HashMap::new();
    for seq in &summary.tool_sequences {
        for tool in seq.split(" → ") {
            *tool_counts.entry(tool.to_string()).or_insert(0) += 1;
        }
    }
    let mut tool_vec: Vec<(String, usize)> = tool_counts.into_iter().collect();
    tool_vec.sort_by(|a, b| b.1.cmp(&a.1));
    let dominant_tools: Vec<String> = tool_vec.into_iter().take(5).map(|(name, _)| name).collect();

    // Tags
    let mut tags = Vec::new();
    if prompt_count > 20 {
        tags.push("high-activity".to_string());
    }
    if summary.static_signals.has_directive {
        tags.push("has-directives".to_string());
    }
    if summary.static_signals.has_correction {
        tags.push("has-corrections".to_string());
    }
    if summary.files_mutated.len() > 10 {
        tags.push("many-file-changes".to_string());
    }
    if summary.static_signals.workflow_complexity == "complex" {
        tags.push("complex-workflow".to_string());
    }

    CacheSessionMeta {
        id: session_id.to_string(),
        file: session_file
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string(),
        file_size,
        prompt_count,
        tool_use_count: summary.tool_use_count,
        first_timestamp,
        last_timestamp,
        duration_minutes,
        dominant_tools,
        tags,
    }
}

// --- Analysis snapshot ---

fn generate_analysis_snapshot(
    sessions: &[(String, Vec<SessionEntry>)],
    history_entries: &[HistoryEntry],
    depth_config: &DepthConfig,
    depth: &AnalysisDepth,
    threshold: usize,
    top: usize,
    decay: bool,
    project_path: &str,
    cache_dir: &Path,
) -> Result<()> {
    let workflow_result = analyze_workflows(sessions, threshold, top, 2, 5);
    let prompt_result = analyze_prompts(history_entries, decay, 14.0);
    let skill_result = analyze_tacit_knowledge(history_entries, threshold, top, depth_config);

    let snapshot = serde_json::json!({
        "analyzedAt": Utc::now().to_rfc3339(),
        "depth": depth.to_string(),
        "project": project_path,
        "promptAnalysis": serde_json::to_value(&prompt_result)?,
        "workflowAnalysis": serde_json::to_value(&workflow_result)?,
        "tacitKnowledge": serde_json::to_value(&skill_result)?,
    });

    let json = serde_json::to_string_pretty(&snapshot)?;
    fs::write(cache_dir.join("analysis-snapshot.json"), &json)?;

    Ok(())
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        let truncated: String = s.chars().take(max_chars - 3).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}
