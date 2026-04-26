mod app;
mod cli;
mod ui;

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = cli::Cli::parse();
    println!("{:#?}", args);
    Ok(())
}
