use anyhow::{Context, Result};
use crate::parsers::{parse_session, list_sessions, resolve_project_path, adapt_to_history_entries};
use crate::analyzers::analyze_tacit_knowledge;

pub fn run(
    threshold: usize,
    top: usize,
    project_path: &str,
    report: bool,
    clustering: bool,
    similarity: f64,
) -> Result<()> {
    // Resolve project path
    let resolved_path = resolve_project_path(project_path)
        .with_context(|| format!(
            "Project not found: {}\nExpected to find encoded directory under ~/.claude/projects/",
            project_path
        ))?;

    eprintln!("Analyzing tacit knowledge in: {}", resolved_path.display());

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

    // Adapt to history entries
    let history_entries = adapt_to_history_entries(&sessions, project_path);

    // Analyze tacit knowledge
    let result = analyze_tacit_knowledge(&history_entries, threshold, top, clustering, similarity);

    // Output results
    println!("\n=== Tacit Knowledge Analysis ===\n");
    println!("Total prompts: {}", result.total);
    println!("Detected patterns: {}\n", result.patterns.len());

    if result.patterns.is_empty() {
        println!("No tacit knowledge patterns found with threshold >= {}", threshold);
        return Ok(());
    }

    println!("Top Tacit Knowledge Patterns:\n");
    println!("{:<4} {:<30} {:<12} {:<8} {:<10}", "#", "Pattern", "Type", "Count", "Confidence");
    println!("{}", "-".repeat(70));

    for (i, pattern) in result.patterns.iter().enumerate() {
        let truncated = if pattern.pattern.chars().count() > 30 {
            let s: String = pattern.pattern.chars().take(27).collect();
            format!("{}...", s)
        } else {
            pattern.pattern.clone()
        };

        // Pad manually to handle multi-byte UTF-8 chars correctly
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

    // Show examples for top patterns
    println!("\n\nExample Prompts for Top Patterns:\n");
    for (i, pattern) in result.patterns.iter().take(3).enumerate() {
        println!("{}. {} ({}x, {:.0}% confidence)",
            i + 1,
            pattern.pattern,
            pattern.count,
            pattern.confidence * 100.0
        );
        println!("   Type: {}", pattern.pattern_type);
        println!("   Examples:");
        for (j, example) in pattern.examples.iter().take(3).enumerate() {
            // Replace newlines to keep output on one line
            let oneline = example.replace('\n', " ");
            let truncated = if oneline.chars().count() > 70 {
                let s: String = oneline.chars().take(67).collect();
                format!("{}...", s)
            } else {
                oneline
            };
            println!("     {}. {}", j + 1, truncated);
        }
        println!();
    }

    if report {
        eprintln!("Markdown report generation not yet implemented.");
    }

    Ok(())
}
