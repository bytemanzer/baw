use std::path::PathBuf;
use std::process::exit;

use baw_asset_extractor::AssetFinder;
use baw_asset_extractor::file::FileAssetFinder;
use baw_asset_extractor::meg::MegAssetFinder;
use baw_asset_extractor::models::ModelVisitor;
use clap::Parser;

/// Extracts content from Empire at War that is needed for `baw` game.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Source directory to scan for config files. This should be the path to the Empire at War
    /// GameData folder.
    #[arg(short, long, alias = "source")]
    src: PathBuf,

    /// "assets" directory to extract to. Paths relative to the GameData folder will be made
    /// relative to this desintation directory.
    #[arg(short, long, alias = "destination")]
    dest: PathBuf,
}

fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let finder = FileAssetFinder::new(&args.src).chain(MegAssetFinder::new(&args.src));
    let mut models = ModelVisitor::new(&args.dest);

    if let Err(e) = finder.find_assets(&mut models) {
        eprintln!("Error processing game assets: {e}");
        exit(1);
    }
}
