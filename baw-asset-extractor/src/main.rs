use std::path::PathBuf;

use clap::Parser;

mod asset;

/// Extracts content from Empire at War that is needed for `baw` game.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Source directory to scan for config files. This should be the path to the Empire at War
    /// GameData folder.
    #[arg(short, long, alias="source")]
    src: PathBuf,

    /// "assets" directory to extract to. Paths relative to the GameData folder will be made
    /// relative to this desintation directory.
    #[arg(short, long, alias="destination")]
    dest: PathBuf,
}

fn main() {
    let args = Args::parse();

}
