use clap::Parser;
use anyhow::{Context, Result};
use chrono::NaiveDate;

mod commands;
mod parsers;
mod analyzers;
mod tokenizer;
mod types;

use analyzers::{AnalysisDepth, StopwordSet, TuningConfig};
use commands::analyze::{AnalysisScope, AnalysisFocus};

#[derive(Parser)]
#[command(name = "suggest-workflow")]
#[command(version = "2.0.0")]
#[command(about = "Analyze Claude session patterns — structural statistics extraction for LLM interpretation")]
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

    /// Path to a JSON tuning config file (overrides default magic numbers).
    /// Use --tuning-defaults to print the default template.
    #[arg(long)]
    tuning: Option<String>,

    /// Print default tuning config as JSON and exit
    #[arg(long)]
    tuning_defaults: bool,

    /// Override: BM25 k1 parameter (term-frequency saturation)
    #[arg(long)]
    bm25_k1: Option<f64>,

    /// Override: BM25 b parameter (document-length normalization)
    #[arg(long)]
    bm25_b: Option<f64>,

    /// Override: time window in minutes for workflow sequence splitting
    #[arg(long)]
    time_window: Option<u64>,

    /// Override: half-life in days for temporal decay
    #[arg(long)]
    decay_half_life: Option<f64>,

    /// Override: z-score threshold for outlier detection
    #[arg(long)]
    z_threshold: Option<f64>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // --tuning-defaults: print template and exit
    if cli.tuning_defaults {
        TuningConfig::print_defaults();
        return Ok(());
    }

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

    // Build tuning config: file → defaults → CLI overrides
    let mut tuning = match &cli.tuning {
        Some(path) => TuningConfig::load_from_file(std::path::Path::new(path))
            .map_err(|e| anyhow::anyhow!(e))?,
        None => TuningConfig::default(),
    };
    tuning.apply_overrides(
        cli.bm25_k1,
        cli.bm25_b,
        cli.time_window,
        cli.decay_half_life,
        cli.z_threshold,
    );

    if cli.cache {
        return commands::cache::run(
            &project_path,
            &depth,
            cli.threshold,
            cli.top,
            cli.decay,
            &stopwords,
            &tuning,
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
        &tuning,
    )
}
