mod analyzers;
mod commands;
mod db;
mod parsers;
mod tokenizer;
mod types;

fn main() -> anyhow::Result<()> {
    commands::run()
}
