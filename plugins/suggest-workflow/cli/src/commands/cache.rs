use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Utc};
use crate::types::*;
use crate::db::repository::QueryRepository;
use crate::parsers::{
    parse_session, list_sessions, resolve_project_path,
    adapt_to_history_entries, extract_tool_sequence,
};
use crate::analyzers::{
    analyze_workflows, analyze_prompts, analyze_tacit_knowledge,
    build_transition_matrix, analyze_repetition, analyze_trends,
    analyze_files, link_sessions, build_dependency_graph,
    AnalysisDepth, DepthConfig, StopwordSet, TuningConfig,
};
use crate::analyzers::tool_classifier::classify_tool;

const CACHE_VERSION: &str = "2.0.0";
const CACHE_DIR_NAME: &str = "suggest-workflow-cache";

/// Generate cache files for a project.
/// All summaries are regenerated every time to ensure consistency
/// with the current code version (no stale cache risk).
/// Outputs the cache directory path to stdout.
pub fn run(
    project_path: &str,
    depth: &AnalysisDepth,
    threshold: usize,
    top: usize,
    decay: bool,
    stopwords: &StopwordSet,
    tuning: &TuningConfig,
    db: Option<&dyn QueryRepository>,
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

    let session_files = list_sessions(&resolved_path)?;
    if session_files.is_empty() {
        eprintln!("No sessions found.");
        return Ok(());
    }

    let mut session_metas: Vec<CacheSessionMeta> = Vec::new();
    let mut all_sessions: Vec<(String, Vec<SessionEntry>)> = Vec::new();
    let mut total_prompts = 0;

    for session_file in &session_files {
        let session_id = session_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let file_size = fs::metadata(session_file)
            .map(|m| m.len())
            .unwrap_or(0);

        let entries = parse_session(session_file)?;

        // Always regenerate summary (no incremental cache)
        let summary = generate_session_summary(&session_id, &entries, project_path);
        let summary_path = sessions_dir.join(format!("{}.summary.json", session_id));
        let json = serde_json::to_string_pretty(&summary)?;
        fs::write(&summary_path, &json)?;

        let meta = build_meta_from_summary(&session_id, session_file, file_size, &summary);
        total_prompts += meta.prompt_count;
        session_metas.push(meta);
        all_sessions.push((session_id, entries));
    }

    eprintln!("Generated {} session summaries", session_metas.len());

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
        stopwords,
        tuning,
        db,
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

// --- Session summary generation (rule-free) ---

fn generate_session_summary(
    session_id: &str,
    entries: &[SessionEntry],
    project_path: &str,
) -> SessionSummary {
    let sessions = vec![(session_id.to_string(), entries.to_vec())];
    let history_entries = adapt_to_history_entries(&sessions, project_path);

    // Prompts — no type classification (delegated to Phase 2 LLM)
    let prompts: Vec<SummaryPrompt> = history_entries
        .iter()
        .map(|e| SummaryPrompt {
            text: e.display.clone(),
            timestamp: e.timestamp,
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

    // Mutated files
    let files_mutated = extract_mutated_files(&tool_uses);

    // Per-file edit counts (pure counting)
    let mut file_edit_counts: HashMap<String, usize> = HashMap::new();
    for tool in &tool_uses {
        if matches!(tool.name.as_str(), "Edit" | "Write" | "NotebookEdit") {
            if let Some(input) = &tool.input {
                let path = input
                    .get("file_path")
                    .or_else(|| input.get("notebook_path"))
                    .and_then(|v| v.as_str());
                if let Some(p) = path {
                    *file_edit_counts.entry(p.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    let mut file_edit_vec: Vec<(String, usize)> = file_edit_counts.into_iter().collect();
    file_edit_vec.sort_by(|a, b| b.1.cmp(&a.1));

    // Tool transition counts within session
    let mut transitions: HashMap<(String, String), usize> = HashMap::new();
    for pair in classified.windows(2) {
        *transitions
            .entry((pair[0].clone(), pair[1].clone()))
            .or_insert(0) += 1;
    }
    let mut transition_vec: Vec<(String, String, usize)> = transitions
        .into_iter()
        .map(|((from, to), count)| (from, to, count))
        .collect();
    transition_vec.sort_by(|a, b| b.2.cmp(&a.2));

    // Max consecutive same tool
    let max_consecutive = max_consecutive_same(&classified);

    // Unique tools
    let unique_tools: HashSet<&str> = classified.iter().map(|s| s.as_str()).collect();

    // Average prompt length
    let avg_prompt_length = if prompts.is_empty() {
        0.0
    } else {
        let total_len: usize = prompts.iter().map(|p| p.text.chars().count()).sum();
        total_len as f64 / prompts.len() as f64
    };

    // Pure quantitative stats
    let stats = SessionStats {
        prompt_count: prompts.len(),
        unique_tool_count: unique_tools.len(),
        total_tool_uses: tool_use_count,
        files_edited_count: files_mutated.len(),
        avg_prompt_length: (avg_prompt_length * 10.0).round() / 10.0,
        max_consecutive_same_tool: max_consecutive,
        tool_transitions: transition_vec,
        file_edit_counts: file_edit_vec,
    };

    SessionSummary {
        id: session_id.to_string(),
        prompts,
        tool_use_count,
        tool_sequences,
        files_mutated,
        stats,
    }
}

fn max_consecutive_same(tools: &[String]) -> usize {
    if tools.is_empty() {
        return 0;
    }
    let mut max_count = 1;
    let mut current_count = 1;
    for i in 1..tools.len() {
        if tools[i] == tools[i - 1] {
            current_count += 1;
            if current_count > max_count {
                max_count = current_count;
            }
        } else {
            current_count = 1;
        }
    }
    max_count
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

// --- Meta builder (quantitative tags only) ---

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

    // Quantitative tags — based on numbers only, no keyword matching
    let mut tags = Vec::new();
    if prompt_count > 20 {
        tags.push("high-activity".to_string());
    }
    if summary.files_mutated.len() > 10 {
        tags.push("many-file-changes".to_string());
    }
    if summary.stats.unique_tool_count > 8 {
        tags.push("complex-workflow".to_string());
    }
    if summary.stats.max_consecutive_same_tool >= 5 {
        tags.push("high-repetition".to_string());
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

// --- Analysis snapshot (includes new statistical analyzers) ---

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
    stopwords: &StopwordSet,
    tuning: &TuningConfig,
    db: Option<&dyn QueryRepository>,
) -> Result<()> {
    // Complex analyses (always in-memory — no DB equivalent yet)
    let workflow_result = analyze_workflows(sessions, threshold, top, tuning.min_seq_length, tuning.max_seq_length, tuning.time_window_minutes);
    let prompt_result = analyze_prompts(history_entries, decay, tuning.decay_half_life_days);
    let skill_result = analyze_tacit_knowledge(history_entries, threshold, top, depth_config, decay, tuning, stopwords);
    let dep_graph_result = build_dependency_graph(sessions, top, tuning);

    // Statistical analyses: use DB queries when available, fall back to in-memory
    let (transition_val, repetition_val, trend_val, file_val, link_val) = if let Some(repo) = db {
        let transitions = repo.execute_sql(
            "SELECT from_tool, to_tool, count, ROUND(probability, 4) AS probability \
             FROM tool_transitions ORDER BY count DESC",
        )?;
        let trends = repo.execute_sql(
            "SELECT week_start, tool_name, count, session_count \
             FROM weekly_buckets ORDER BY week_start, count DESC",
        )?;
        let files = repo.execute_sql(
            "SELECT file_path, edit_count, session_count \
             FROM file_hotspots ORDER BY edit_count DESC",
        )?;
        let links = repo.execute_sql(
            "SELECT session_a, session_b, shared_files, \
                    ROUND(overlap_ratio, 3) AS overlap_ratio, time_gap_minutes \
             FROM session_links ORDER BY overlap_ratio DESC",
        )?;
        let repetition = repo.execute_sql(
            "SELECT session_id, classified_name AS tool, \
                    COUNT(*) AS count \
             FROM tool_uses \
             GROUP BY session_id, classified_name \
             ORDER BY count DESC",
        )?;

        eprintln!("Analysis: using SQLite DB for statistical sections");
        (transitions, repetition, trends, files, links)
    } else {
        let transition_result = build_transition_matrix(sessions);
        let repetition_result = analyze_repetition(sessions, tuning);
        let trend_result = analyze_trends(sessions, history_entries, tuning);
        let file_result = analyze_files(sessions, top);
        let link_result = link_sessions(sessions);

        (
            serde_json::to_value(&transition_result)?,
            serde_json::to_value(&repetition_result)?,
            serde_json::to_value(&trend_result)?,
            serde_json::to_value(&file_result)?,
            serde_json::to_value(&link_result)?,
        )
    };

    let snapshot = serde_json::json!({
        "analyzedAt": Utc::now().to_rfc3339(),
        "depth": depth.to_string(),
        "project": project_path,
        "cacheVersion": CACHE_VERSION,
        "dataSource": if db.is_some() { "sqlite" } else { "legacy" },
        "tuning": serde_json::to_value(tuning).unwrap_or_default(),

        // Complex analyses (always in-memory)
        "promptAnalysis": serde_json::to_value(&prompt_result)?,
        "workflowAnalysis": serde_json::to_value(&workflow_result)?,
        "tacitKnowledge": serde_json::to_value(&skill_result)?,
        "dependencyGraph": serde_json::to_value(&dep_graph_result)?,

        // Statistical analyses (DB-backed when available)
        "toolTransitions": transition_val,
        "repetitionStats": repetition_val,
        "weeklyTrends": trend_val,
        "fileAnalysis": file_val,
        "sessionLinks": link_val,
    });

    let json = serde_json::to_string_pretty(&snapshot)?;
    fs::write(cache_dir.join("analysis-snapshot.json"), &json)?;

    Ok(())
}
