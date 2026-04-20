use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
struct Args {
    /// The ALO file to parse.
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let _content = std::fs::read(&args.file)?;
    Ok(())
}
