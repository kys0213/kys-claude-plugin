use crate::analyzers::{
    analyze_prompts, analyze_tacit_knowledge, analyze_workflows, build_dependency_graph,
    AnalysisDepth, DepthConfig, StopwordSet, TuningConfig,
};
use crate::parsers::projects::list_projects;
use crate::parsers::{
    adapt_to_history_entries, list_sessions, parse_session, resolve_project_path,
};
use anyhow::{Context, Result};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

/// Analysis scope
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalysisScope {
    /// Single project analysis
    Project,
    /// Cross-project global analysis
    Global,
}

impl std::str::FromStr for AnalysisScope {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "project" => Ok(AnalysisScope::Project),
            "global" => Ok(AnalysisScope::Global),
            _ => Err(format!("invalid scope '{}': expected project or global", s)),
        }
    }
}

/// Analysis focus
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnalysisFocus {
    /// Run all analyses
    All,
    /// Workflow/tool sequence analysis only
    Workflow,
    /// Tacit knowledge/skill analysis only
    Skill,
}

impl std::str::FromStr for AnalysisFocus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "all" => Ok(AnalysisFocus::All),
            "workflow" => Ok(AnalysisFocus::Workflow),
            "skill" => Ok(AnalysisFocus::Skill),
            _ => Err(format!(
                "invalid focus '{}': expected all, workflow, or skill",
                s
            )),
        }
    }
}

/// Unified analysis entry point
pub fn run(
    scope: AnalysisScope,
    depth: AnalysisDepth,
    focus: AnalysisFocus,
    project_path: &str,
    threshold: usize,
    top: usize,
    format: &str,
    decay: bool,
    date_range: Option<(i64, i64)>,
    stopwords: &StopwordSet,
    tuning: &TuningConfig,
) -> Result<()> {
    let depth_config = depth.resolve();

    match scope {
        AnalysisScope::Project => run_project_analysis(
            project_path,
            &depth_config,
            &depth,
            focus,
            threshold,
            top,
            format,
            decay,
            date_range,
            stopwords,
            tuning,
        ),
        AnalysisScope::Global => run_global_analysis(
            &depth_config,
            &depth,
            focus,
            threshold,
            top,
            format,
            decay,
            date_range,
            stopwords,
            tuning,
        ),
    }
}

/// Filter history entries by date range
fn apply_date_filter(
    entries: Vec<crate::types::HistoryEntry>,
    date_range: Option<(i64, i64)>,
) -> Vec<crate::types::HistoryEntry> {
    match date_range {
        Some((since, until)) => entries
            .into_iter()
            .filter(|e| e.timestamp >= since && e.timestamp <= until)
            .collect(),
        None => entries,
    }
}

/// Single-project analysis
fn run_project_analysis(
    project_path: &str,
    depth_config: &DepthConfig,
    depth: &AnalysisDepth,
    focus: AnalysisFocus,
    threshold: usize,
    top: usize,
    format: &str,
    decay: bool,
    date_range: Option<(i64, i64)>,
    stopwords: &StopwordSet,
    tuning: &TuningConfig,
) -> Result<()> {
    let resolved_path = resolve_project_path(project_path).with_context(|| {
        format!(
            "Project not found: {}\nExpected to find encoded directory under ~/.claude/projects/",
            project_path
        )
    })?;

    eprintln!("Analyzing project: {}", resolved_path.display());
    eprintln!("Depth: {} | Focus: {:?}", depth, focus);

    let (sessions, history_entries) = load_sessions_from_dir(&resolved_path, project_path)?;
    let history_entries = apply_date_filter(history_entries, date_range);

    if sessions.is_empty() {
        eprintln!("No session files found.");
        std::process::exit(2);
    }

    eprintln!(
        "Loaded {} sessions ({} prompts after date filter)",
        sessions.len(),
        history_entries.len()
    );

    if format == "json" {
        print_json_output(
            &sessions,
            &history_entries,
            depth_config,
            depth,
            focus,
            threshold,
            top,
            decay,
            project_path,
            None,
            stopwords,
            tuning,
        )
    } else {
        print_text_output(
            &sessions,
            &history_entries,
            depth_config,
            depth,
            focus,
            threshold,
            top,
            decay,
            None,
            stopwords,
            tuning,
        )
    }
}

/// Global cross-project analysis
fn run_global_analysis(
    depth_config: &DepthConfig,
    depth: &AnalysisDepth,
    focus: AnalysisFocus,
    threshold: usize,
    top: usize,
    format: &str,
    decay: bool,
    date_range: Option<(i64, i64)>,
    stopwords: &StopwordSet,
    tuning: &TuningConfig,
) -> Result<()> {
    let project_dirs = list_projects(None)?;
    if project_dirs.is_empty() {
        eprintln!("No projects found in ~/.claude/projects/");
        std::process::exit(2);
    }

    eprintln!("Global analysis: {} projects found", project_dirs.len());
    eprintln!("Depth: {} | Focus: {:?}", depth, focus);

    let home = std::env::var("HOME").context("HOME not set")?;
    let projects_base = PathBuf::from(&home).join(".claude").join("projects");

    let mut all_sessions = Vec::new();
    let mut all_history = Vec::new();
    let mut project_stats: Vec<ProjectStats> = Vec::new();

    for dir_name in &project_dirs {
        let project_dir = projects_base.join(dir_name);
        let project_label = decode_project_name(dir_name);

        match load_sessions_from_dir(&project_dir, &project_label) {
            Ok((sessions, history)) => {
                let prompt_count = history.len();
                if prompt_count > 0 {
                    project_stats.push(ProjectStats {
                        name: project_label.clone(),
                        prompt_count,
                        session_count: sessions.len(),
                    });
                }
                all_sessions.extend(sessions);
                all_history.extend(history);
            }
            Err(e) => {
                eprintln!("Warning: skipping {}: {}", dir_name, e);
            }
        }
    }

    let all_history = apply_date_filter(all_history, date_range);

    if all_history.is_empty() {
        eprintln!("No prompts found across any projects.");
        std::process::exit(2);
    }

    eprintln!(
        "Loaded {} prompts from {} projects ({} sessions)",
        all_history.len(),
        project_stats.len(),
        all_sessions.len(),
    );

    let global_info = Some(GlobalInfo {
        project_count: project_stats.len(),
        total_prompts: all_history.len(),
        projects: project_stats,
    });

    if format == "json" {
        print_json_output(
            &all_sessions,
            &all_history,
            depth_config,
            depth,
            focus,
            threshold,
            top,
            decay,
            "global",
            global_info.as_ref(),
            stopwords,
            tuning,
        )
    } else {
        print_text_output(
            &all_sessions,
            &all_history,
            depth_config,
            depth,
            focus,
            threshold,
            top,
            decay,
            global_info.as_ref(),
            stopwords,
            tuning,
        )
    }
}

// --- Data loading helpers ---

struct ProjectStats {
    name: String,
    prompt_count: usize,
    session_count: usize,
}

struct GlobalInfo {
    project_count: usize,
    total_prompts: usize,
    projects: Vec<ProjectStats>,
}

/// Load sessions and history entries from a project directory.
/// Uses rayon for parallel JSONL parsing across session files.
fn load_sessions_from_dir(
    project_dir: &Path,
    project_label: &str,
) -> Result<(
    Vec<(String, Vec<crate::types::SessionEntry>)>,
    Vec<crate::types::HistoryEntry>,
)> {
    let session_files = list_sessions(project_dir)?;

    let sessions: Vec<(String, Vec<crate::types::SessionEntry>)> = session_files
        .par_iter()
        .filter_map(|session_file| {
            let entries = parse_session(session_file).ok()?;
            let session_id = session_file
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            Some((session_id, entries))
        })
        .collect();

    let history_entries = adapt_to_history_entries(&sessions, project_label);
    Ok((sessions, history_entries))
}

fn decode_project_name(encoded: &str) -> String {
    // Claude encodes paths by replacing "/" with "-", but this is lossy:
    //   /home/user/my-project → -home-user-my-project
    // We cannot distinguish original hyphens from encoded slashes.
    // Use the encoded name directly as a display label (stripping leading dash).
    if encoded.starts_with('-') {
        encoded[1..].to_string()
    } else {
        encoded.to_string()
    }
}

// --- Output formatting ---

fn print_text_output(
    sessions: &[(String, Vec<crate::types::SessionEntry>)],
    history_entries: &[crate::types::HistoryEntry],
    depth_config: &DepthConfig,
    depth: &AnalysisDepth,
    focus: AnalysisFocus,
    threshold: usize,
    top: usize,
    decay: bool,
    global_info: Option<&GlobalInfo>,
    stopwords: &StopwordSet,
    tuning: &TuningConfig,
) -> Result<()> {
    // Header
    if let Some(info) = global_info {
        println!(
            "\n=== Global Analysis ({} projects, {} prompts) ===",
            info.project_count, info.total_prompts
        );
    } else {
        println!("\n=== Project Analysis ===");
    }
    println!(
        "Depth: {} | Multi-query: {:?}\n",
        depth, depth_config.multi_query_strategy
    );

    // Workflow analysis
    if focus == AnalysisFocus::All || focus == AnalysisFocus::Workflow {
        let workflow_result = analyze_workflows(
            sessions,
            threshold,
            top,
            tuning.min_seq_length,
            tuning.max_seq_length,
            tuning.time_window_minutes,
        );
        let prompt_result = analyze_prompts(history_entries, decay, tuning.decay_half_life_days);

        println!("--- Workflow Analysis ---\n");
        println!(
            "Total prompts: {} | Unique: {}",
            prompt_result.total, prompt_result.unique
        );
        if let Some(start) = &prompt_result.start_date {
            println!(
                "Period: {} ~ {}",
                start,
                prompt_result.end_date.as_deref().unwrap_or("N/A")
            );
        }

        if !prompt_result.top_prompts.is_empty() {
            println!("\nTop Prompts:");
            for (i, p) in prompt_result.top_prompts.iter().take(top).enumerate() {
                let display: String = p.prompt.chars().take(60).collect();
                println!("  {}. [{}x] {}", i + 1, p.count, display);
            }
        }

        println!(
            "\nTotal sequences: {} | Unique: {}",
            workflow_result.total_sequences, workflow_result.unique_sequences
        );
        if !workflow_result.top_sequences.is_empty() {
            println!("\nTop Tool Sequences:");
            for (i, seq) in workflow_result.top_sequences.iter().enumerate() {
                println!("  {}. [{}x] {}", i + 1, seq.count, seq.tools.join(" → "));
            }
        }

        if !workflow_result.tool_usage_stats.is_empty() {
            println!("\nTool Usage:");
            for (i, (tool, count)) in workflow_result.tool_usage_stats.iter().take(10).enumerate() {
                println!("  {}. {}: {}x", i + 1, tool, count);
            }
        }
        // Dependency graph
        let dep_graph = build_dependency_graph(sessions, top, tuning);
        println!("--- Tool Dependency Graph ---\n");
        println!(
            "Nodes: {} | Edges: {} | Cycles: {}\n",
            dep_graph.nodes.len(),
            dep_graph.edges.len(),
            dep_graph.cycles.len()
        );

        if !dep_graph.nodes.is_empty() {
            println!("Top Nodes (by usage):");
            println!(
                "{:<20} {:<8} {:<8} {:<8} {:<8} {:<10} {:<10}",
                "Tool", "Uses", "Fanout", "Fanin", "AvgPos", "Entry%", "Terminal%"
            );
            println!("{}", "-".repeat(72));
            for node in dep_graph.nodes.iter().take(top) {
                println!(
                    "{:<20} {:<8} {:<8} {:<8} {:<8.2} {:<10.0} {:<10.0}",
                    truncate_str(&node.tool, 20),
                    node.total_uses,
                    node.fanout,
                    node.fanin,
                    node.avg_position,
                    node.entry_rate * 100.0,
                    node.terminal_rate * 100.0
                );
            }
            println!();
        }

        if !dep_graph.edges.is_empty() {
            println!("Top Edges (by frequency):");
            println!(
                "{:<20} {:<20} {:<6} {:<8} {:<8} {:<10}",
                "From", "To", "Count", "P(→)", "P(←)", "Commit%"
            );
            println!("{}", "-".repeat(72));
            for edge in dep_graph.edges.iter().take(top) {
                println!(
                    "{:<20} {:<20} {:<6} {:<8.2} {:<8.2} {:<10.0}",
                    truncate_str(&edge.from, 20),
                    truncate_str(&edge.to, 20),
                    edge.count,
                    edge.probability,
                    edge.reverse_probability,
                    edge.commit_reachable_rate * 100.0
                );
            }
            println!();
        }

        if !dep_graph.cycles.is_empty() {
            println!("Detected Cycles:");
            for (i, cycle) in dep_graph.cycles.iter().take(5).enumerate() {
                println!(
                    "  {}. [{}x, avg {:.1} iter] {}",
                    i + 1,
                    cycle.occurrence_count,
                    cycle.avg_iterations,
                    cycle.tools.join(" ↔ ")
                );
            }
            println!();
        }

        if !dep_graph.critical_paths.is_empty() {
            println!("Critical Paths:");
            for (i, path) in dep_graph.critical_paths.iter().take(5).enumerate() {
                println!(
                    "  {}. [{}x, {:.0}% commit] {}",
                    i + 1,
                    path.frequency,
                    path.commit_rate * 100.0,
                    path.path.join(" → ")
                );
            }
            println!();
        }

        println!();
    }

    // Skill/tacit knowledge analysis
    if focus == AnalysisFocus::All || focus == AnalysisFocus::Skill {
        let skill_result = analyze_tacit_knowledge(
            history_entries,
            threshold,
            top,
            depth_config,
            decay,
            tuning,
            stopwords,
        );

        println!("--- Tacit Knowledge Analysis ---\n");
        println!("Detected patterns: {}\n", skill_result.patterns.len());

        if skill_result.patterns.is_empty() {
            println!("No patterns found with threshold >= {}", threshold);
        } else {
            println!(
                "{:<4} {:<30} {:<12} {:<8} {:<10}",
                "#", "Pattern", "Type", "Count", "Confidence"
            );
            println!("{}", "-".repeat(70));

            for (i, pattern) in skill_result.patterns.iter().enumerate() {
                let truncated = if pattern.pattern.chars().count() > 30 {
                    let s: String = pattern.pattern.chars().take(27).collect();
                    format!("{}...", s)
                } else {
                    pattern.pattern.clone()
                };

                let char_count = truncated.chars().count();
                let padded = if char_count < 30 {
                    format!("{}{}", truncated, " ".repeat(30 - char_count))
                } else {
                    truncated
                };

                println!(
                    "{:<4} {} {:<12} {:<8} {:<10.0}%",
                    i + 1,
                    padded,
                    pattern.pattern_type,
                    pattern.count,
                    pattern.confidence * 100.0
                );
            }

            // Examples for top patterns
            println!("\n\nExample Prompts:\n");
            for (i, pattern) in skill_result.patterns.iter().take(3).enumerate() {
                println!(
                    "{}. {} ({}x, {:.0}% confidence)",
                    i + 1,
                    pattern.pattern,
                    pattern.count,
                    pattern.confidence * 100.0
                );
                println!("   Type: {}", pattern.pattern_type);
                println!("   BM25: {:.2}", pattern.bm25_score);
                for (j, example) in pattern.examples.iter().take(3).enumerate() {
                    let oneline = example.replace('\n', " ");
                    let display: String = oneline.chars().take(70).collect();
                    println!("     {}. {}", j + 1, display);
                }
                println!();
            }
        }
    }

    // Global project breakdown
    if let Some(info) = global_info {
        println!("--- Project Breakdown ---\n");
        let mut sorted = info.projects.iter().collect::<Vec<_>>();
        sorted.sort_by(|a, b| b.prompt_count.cmp(&a.prompt_count));
        for stat in sorted.iter().take(20) {
            let short_name: String = stat
                .name
                .split('/')
                .last()
                .unwrap_or(&stat.name)
                .to_string();
            println!(
                "  {}: {} prompts, {} sessions",
                short_name, stat.prompt_count, stat.session_count
            );
        }
        println!();
    }

    Ok(())
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max - 2).collect();
        format!("{}..", truncated)
    } else {
        s.to_string()
    }
}

fn print_json_output(
    sessions: &[(String, Vec<crate::types::SessionEntry>)],
    history_entries: &[crate::types::HistoryEntry],
    depth_config: &DepthConfig,
    depth: &AnalysisDepth,
    focus: AnalysisFocus,
    threshold: usize,
    top: usize,
    decay: bool,
    project_path: &str,
    global_info: Option<&GlobalInfo>,
    stopwords: &StopwordSet,
    tuning: &TuningConfig,
) -> Result<()> {
    let mut output = serde_json::json!({
        "analyzedAt": chrono::Utc::now().to_rfc3339(),
        "depth": depth.to_string(),
        "scope": if global_info.is_some() { "global" } else { "project" },
        "tuning": serde_json::to_value(tuning).unwrap_or_default(),
    });

    if global_info.is_none() {
        output["projectPath"] = serde_json::json!(project_path);
    }

    if let Some(info) = global_info {
        output["globalSummary"] = serde_json::json!({
            "projectCount": info.project_count,
            "totalPrompts": info.total_prompts,
            "projects": info.projects.iter().map(|p| serde_json::json!({
                "name": p.name,
                "promptCount": p.prompt_count,
                "sessionCount": p.session_count,
            })).collect::<Vec<_>>(),
        });
    }

    if focus == AnalysisFocus::All || focus == AnalysisFocus::Workflow {
        let workflow_result = analyze_workflows(
            sessions,
            threshold,
            top,
            tuning.min_seq_length,
            tuning.max_seq_length,
            tuning.time_window_minutes,
        );
        let prompt_result = analyze_prompts(history_entries, decay, tuning.decay_half_life_days);
        let dep_graph = build_dependency_graph(sessions, top, tuning);
        output["promptAnalysis"] = serde_json::to_value(&prompt_result)?;
        output["workflowAnalysis"] = serde_json::to_value(&workflow_result)?;
        output["dependencyGraph"] = serde_json::to_value(&dep_graph)?;
    }

    if focus == AnalysisFocus::All || focus == AnalysisFocus::Skill {
        let skill_result = analyze_tacit_knowledge(
            history_entries,
            threshold,
            top,
            depth_config,
            decay,
            tuning,
            stopwords,
        );
        output["tacitKnowledge"] = serde_json::to_value(&skill_result)?;
    }

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
