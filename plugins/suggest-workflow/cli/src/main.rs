use clap::Parser;
use anyhow::{Context, Result};

mod commands;
mod parsers;
mod analyzers;
mod tokenizer;
mod types;

use analyzers::AnalysisDepth;
use commands::analyze::{AnalysisScope, AnalysisFocus};

#[derive(Parser)]
#[command(name = "suggest-workflow")]
#[command(version = "1.0.0")]
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let scope: AnalysisScope = cli.scope.parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    let depth: AnalysisDepth = cli.depth.parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    let focus: AnalysisFocus = cli.focus.parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;

    let project_path = match cli.project {
        Some(p) => p,
        None => std::env::current_dir()
            .context("failed to get current directory")?
            .to_string_lossy()
            .to_string(),
    };

    commands::analyze::run(
        scope,
        depth,
        focus,
        &project_path,
        cli.threshold,
        cli.top,
        &cli.format,
        cli.decay,
    )
}
