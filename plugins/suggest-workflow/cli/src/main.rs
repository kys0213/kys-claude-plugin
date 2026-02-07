use clap::{Parser, Subcommand};
use anyhow::Result;

mod commands;
mod parsers;
mod analyzers;
mod tokenizer;
mod types;

#[derive(Parser)]
#[command(name = "suggest-workflow")]
#[command(version = "0.5.0")]
#[command(about = "Analyze Claude session patterns and suggest workflows/skills")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze tool sequences and suggest workflows
    Workflow {
        /// Data source: history or projects
        #[arg(long, default_value = "projects")]
        source: String,

        /// Minimum frequency threshold
        #[arg(long, default_value_t = 5)]
        threshold: usize,

        /// Top N results to show
        #[arg(long, default_value_t = 10)]
        top: usize,

        /// Project path (defaults to current directory)
        #[arg(long)]
        project: Option<String>,

        /// Generate markdown report
        #[arg(long)]
        report: bool,

        /// Output format: text or json
        #[arg(long, default_value = "text")]
        format: String,

        /// Enable gap-tolerant sequence matching
        #[arg(long)]
        gap_tolerant: bool,

        /// Enable temporal decay weighting
        #[arg(long)]
        decay: bool,
    },

    /// Extract tacit knowledge and suggest skills
    Skill {
        /// Minimum frequency threshold
        #[arg(long, default_value_t = 3)]
        threshold: usize,

        /// Top N results to show
        #[arg(long, default_value_t = 10)]
        top: usize,

        /// Project path (defaults to current directory)
        #[arg(long)]
        project: Option<String>,

        /// Generate markdown report
        #[arg(long)]
        report: bool,

        /// Disable similarity clustering
        #[arg(long)]
        no_clustering: bool,

        /// Similarity threshold (0.0-1.0)
        #[arg(long, default_value_t = 0.7)]
        similarity: f64,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Workflow {
            source,
            threshold,
            top,
            project,
            report,
            format,
            gap_tolerant,
            decay,
        } => {
            let project_path = project.unwrap_or_else(|| std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string());

            commands::workflow::run(
                &source,
                threshold,
                top,
                &project_path,
                report,
                &format,
                gap_tolerant,
                decay,
            )
        }
        Commands::Skill {
            threshold,
            top,
            project,
            report,
            no_clustering,
            similarity,
        } => {
            let project_path = project.unwrap_or_else(|| std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .to_string());

            commands::skill::run(
                threshold,
                top,
                &project_path,
                report,
                !no_clustering,
                similarity,
            )
        }
    }
}
