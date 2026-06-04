use atelier::cli::{Atelier, Group};
use clap::Parser;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Atelier::parse();
    match cli.command {
        Group::Autopilot(autopilot_cli) => atelier::autopilot::run(autopilot_cli),
    }
}
