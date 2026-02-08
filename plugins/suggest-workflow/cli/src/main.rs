use clap::Parser;
use anyhow::{Context, Result};
use chrono::NaiveDate;

mod commands;
mod parsers;
mod analyzers;
mod tokenizer;
mod types;

use analyzers::{AnalysisDepth, StopwordSet};
use commands::analyze::{AnalysisScope, AnalysisFocus};

#[derive(Parser)]
#[command(name = "suggest-workflow")]
#[command(version = "1.1.0")]
#[command(about = "Analyze Claude session patterns — unified workflow + skill analysis with multi-query BM25")]
struct Cli {
    /// Analysis scope: project (single) or global (cross-project)
    #[arg(long, default_value = "project")]
    scope: String,

    /// Analysis depth: narrow, normal, or wide
    #[arg(long, default_value = "normal")]
    depth: String,

    /// Analysis focus: all, workflow, or skill
    #[arg(long, default_value = "all")]
    focus: String,

    /// Project path (for scope=project, defaults to current directory)
    #[arg(long)]
    project: Option<String>,

    /// Minimum frequency threshold
    #[arg(long, default_value_t = 3)]
    threshold: usize,

    /// Top N results to show
    #[arg(long, default_value_t = 10)]
    top: usize,

    /// Output format: text or json
    #[arg(long, default_value = "text")]
    format: String,

    /// Enable temporal decay weighting
    #[arg(long)]
    decay: bool,

    /// Filter: only include entries after this date (YYYY-MM-DD)
    #[arg(long)]
    since: Option<String>,

    /// Filter: only include entries before this date (YYYY-MM-DD)
    #[arg(long)]
    until: Option<String>,

    /// Additional words to exclude from analysis (comma-separated).
    /// Merged with built-in defaults and ~/.claude/suggest-workflow/stopwords.json
    #[arg(long, value_delimiter = ',')]
    exclude_words: Vec<String>,

    /// Generate cache files for Claude semantic analysis (Phase 2).
    /// Outputs cache directory path to stdout.
    #[arg(long)]
    cache: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let depth: AnalysisDepth = cli.depth.parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let project_path = match cli.project {
        Some(p) => p,
        None => std::env::current_dir()
            .context("failed to get current directory")?
            .to_string_lossy()
            .to_string(),
    };

    // Parse date range filters
    let since_ms = cli.since.as_deref().map(|s| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis())
            .map_err(|e| anyhow::anyhow!("invalid --since date '{}': {} (expected YYYY-MM-DD)", s, e))
    }).transpose()?;

    let until_ms = cli.until.as_deref().map(|s| {
        NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(|d| d.and_hms_opt(23, 59, 59).unwrap().and_utc().timestamp_millis())
            .map_err(|e| anyhow::anyhow!("invalid --until date '{}': {} (expected YYYY-MM-DD)", s, e))
    }).transpose()?;

    let date_range = match (since_ms, until_ms) {
        (Some(s), Some(u)) => Some((s, u)),
        (Some(s), None) => Some((s, i64::MAX)),
        (None, Some(u)) => Some((i64::MIN, u)),
        (None, None) => None,
    };

    // Build stopword set from defaults + config file + CLI
    let stopwords = StopwordSet::load(&cli.exclude_words);

    if cli.cache {
        return commands::cache::run(
            &project_path,
            &depth,
            cli.threshold,
            cli.top,
            cli.decay,
            &stopwords,
        );
    }

    let scope: AnalysisScope = cli.scope.parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    let focus: AnalysisFocus = cli.focus.parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    commands::analyze::run(
        scope,
        depth,
        focus,
        &project_path,
        cli.threshold,
        cli.top,
        &cli.format,
        cli.decay,
        date_range,
        &stopwords,
    )
}
