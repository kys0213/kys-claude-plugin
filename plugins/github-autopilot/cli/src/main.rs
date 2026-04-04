mod cmd;
mod gh;

use clap::Parser;
use cmd::{Cli, Commands, IssueCommands, PipelineCommands};

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Issue { command } => match command {
            IssueCommands::CheckDup { fingerprint } => cmd::issue::check_dup(&fingerprint),
            IssueCommands::Create(args) => cmd::issue::create(&args),
            IssueCommands::CloseResolved { label_prefix } => {
                cmd::issue::close_resolved(&label_prefix)
            }
        },
        Commands::Pipeline { command } => match command {
            PipelineCommands::Idle { label_prefix } => cmd::pipeline::idle(&label_prefix),
        },
    };

    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("{e:#}");
            std::process::exit(2);
        }
    }
}
