use anyhow::{Context, Result};
use std::path::PathBuf;
use crate::parsers::{parse_session, list_sessions, resolve_project_path, adapt_to_history_entries};
use crate::analyzers::{analyze_workflows, analyze_prompts};

pub fn run(
    source: &str,
    threshold: usize,
    top: usize,
    project_path: &str,
    report: bool,
    format: &str,
    _gap_tolerant: bool,
    decay: bool,
) -> Result<()> {
    match source {
        "history" => {
            eprintln!("History source not yet implemented. Use 'projects' source.");
            std::process::exit(1);
        }
        "projects" => {
            run_projects_analysis(project_path, threshold, top, report, format, decay)
        }
        _ => {
            anyhow::bail!("Invalid source: {}. Must be 'history' or 'projects'.", source);
        }
    }
}

fn run_projects_analysis(
    project_path: &str,
    threshold: usize,
    top: usize,
    report: bool,
    format: &str,
    decay: bool,
) -> Result<()> {
    // Resolve project path
    let resolved_path = resolve_project_path(project_path)
        .with_context(|| format!(
            "Project not found: {}\nExpected to find encoded directory under ~/.claude/projects/",
            project_path
        ))?;

    eprintln!("Analyzing project: {}", resolved_path.display());

    // Load all sessions
    let session_files = list_sessions(&resolved_path)?;
    if session_files.is_empty() {
        eprintln!("No session files found in project directory.");
        std::process::exit(2);
    }

    let mut sessions = Vec::new();
    for session_file in &session_files {
        let entries = parse_session(session_file)?;
        let session_id = session_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        sessions.push((session_id, entries));
    }

    eprintln!("Loaded {} sessions", sessions.len());

    // Analyze workflows
    let workflow_result = analyze_workflows(&sessions, threshold, top, 2, 5);

    // Adapt to history entries for prompt analysis
    let history_entries = adapt_to_history_entries(&sessions, project_path);
    let prompt_result = analyze_prompts(&history_entries, decay, 14.0);

    // Output results
    if format == "json" {
        let output = serde_json::json!({
            "projectPath": project_path,
            "source": "projects",
            "analyzedAt": chrono::Utc::now().to_rfc3339(),
            "promptAnalysis": prompt_result,
            "workflowAnalysis": workflow_result,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print_text_summary(&prompt_result, &workflow_result);
    }

    if report {
        eprintln!("\nMarkdown report generation not yet implemented.");
    }

    Ok(())
}

fn print_text_summary(
    prompt_result: &crate::types::PromptAnalysisResult,
    workflow_result: &crate::types::WorkflowAnalysisResult,
) {
    println!("\n=== Workflow Analysis Summary ===\n");

    println!("Prompt Analysis:");
    println!("  Total prompts: {}", prompt_result.total);
    println!("  Unique prompts: {}", prompt_result.unique);
    if let Some(start) = &prompt_result.start_date {
        println!("  Date range: {} to {}", start, prompt_result.end_date.as_ref().unwrap_or(&"N/A".to_string()));
    }

    println!("\nTop Prompts:");
    for (i, p) in prompt_result.top_prompts.iter().take(10).enumerate() {
        println!("  {}. [{}x] {}", i + 1, p.count, p.prompt.chars().take(60).collect::<String>());
    }

    println!("\n\nWorkflow Analysis:");
    println!("  Total sequences: {}", workflow_result.total_sequences);
    println!("  Unique sequences: {}", workflow_result.unique_sequences);

    println!("\nTop Tool Sequences:");
    for (i, seq) in workflow_result.top_sequences.iter().enumerate() {
        println!("  {}. [{}x] {}", i + 1, seq.count, seq.tools.join(" → "));
    }

    println!("\nTool Usage Stats:");
    for (i, (tool, count)) in workflow_result.tool_usage_stats.iter().take(10).enumerate() {
        println!("  {}. {}: {}x", i + 1, tool, count);
    }
}
