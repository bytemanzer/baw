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

    /// "assets" directory to extract to.
    #[arg(short, long, alias = "destination")]
    dest: PathBuf,

    /// Prefix to place extracted assets under, within the assets directory.
    #[arg(short, long, default_value = "extracted")]
    prefix: PathBuf,
}

fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let mut dest = args.dest;
    dest.push(&args.prefix);

    let finder = FileAssetFinder::new(&args.src).chain(MegAssetFinder::new(&args.src));
    let mut models = ModelVisitor::new(&dest);

    if let Err(e) = finder.find_assets(&mut models) {
        eprintln!("Error processing game assets: {e}");
        exit(1);
    }
}
