use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::exit;

use baw_asset_extractor::assets::FileSystemView;
use baw_asset_extractor::extraction::config::ConfigExtractor;
use baw_asset_extractor::extraction::models::ModelExtractor;
use baw_asset_extractor::processing::{FileWriter, Processor, WriterConfig};
use clap::Parser;
use tracing::{Level, error, info, warn};
use tracing_subscriber::EnvFilter;

/// Extracts content from Empire at War that is needed for `baw` game.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    /// Source directory to scan for config files. This should be the path to the Empire at War
    /// GameData folder.
    #[arg(short, long, alias = "source")]
    src: PathBuf,

    /// Path to a mod folder, which will be layered on top of the base GameData before extracting.
    #[arg(long = "mod", alias = "modpath")]
    modpath: Option<PathBuf>,

    #[command(flatten)]
    mode: Mode,

    /// Prefix to place extracted assets under, within the assets directory.
    ///
    /// The prefix is also applied to relative paths used within the extracted assets.
    #[arg(short, long, default_value = "extracted")]
    prefix: PathBuf,

    /// Number of threads to use.
    num_threads: Option<usize>,
}

#[derive(clap::Args, Debug)]
#[group(required = true, multiple = false)]
struct Mode {
    /// "assets" directory to extract to.
    #[arg(short, long, alias = "destination")]
    dest: Option<PathBuf>,
    /// List all files in the built filesystem view.
    #[arg(short, long, alias = "list-files")]
    list: bool,
}

fn main() {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(Level::WARN.into())
                .with_env_var(EnvFilter::DEFAULT_ENV)
                .from_env_lossy(),
        )
        .init();
    let args = Args::parse();

    let fs = build_fs_view(&args.src, args.modpath.as_deref()).unwrap_or_else(|err| {
        eprintln!("Unable to build filesystem view: {err}");
        exit(1);
    });

    if args.mode.list {
        for asset in fs.assets() {
            println!("{}", asset.path());
        }
        return;
    }

    let write_config = WriterConfig::new(args.mode.dest.unwrap()).set_prefix(&args.prefix);
    let output = FileWriter::new(&write_config);
    let models = ModelExtractor::new(&fs, &output).with_prefix(&args.prefix);
    let configs = ConfigExtractor::new(&fs, &models, &output).with_prefix(&args.prefix);

    let res = Processor::new()
        .num_threads(choose_num_threads(args.num_threads))
        .run(|ctx| configs.extract_data_files(ctx));

    if let Err(err) = res {
        eprintln!("Failed to extract game assets: {err}");
        exit(1);
    }
    info!("Done");
}

/// Build the file system view or the given src and mod paths.
fn build_fs_view(src: &Path, modpath: Option<&Path>) -> anyhow::Result<FileSystemView> {
    let mut fs = FileSystemView::new();
    info!("Finding disk files in {}", src.display());
    fs.overlay_disk_files(src)?;
    if let Some(modpath) = modpath {
        info!("Finding mod files in {}", modpath.display());
        fs.overlay_disk_files(modpath)?;
    }
    info!("Finding mega files");
    fs.overlay_mega_files()?;
    Ok(fs)
}

/// Get the number of threads to use, based on the configured requested number.
fn choose_num_threads(requested: Option<usize>) -> NonZeroUsize {
    if let Some(requested) = requested {
        return NonZeroUsize::new(requested).unwrap_or_else(|| {
            warn!("Requested 0 threads, defaulting to 1 thread");
            return NonZeroUsize::MIN;
        });
    }

    match std::thread::available_parallelism() {
        Ok(available) => {
            // We expect to do blocking work on these threads, since operations are generally either
            // CPU bound or FS bound. FS operations are not reliably async arcoss operating systems,
            // so we just use regular blocking FS ops and allocate a thread pool large enough to
            // handle some blocking within the async task pool.
            available.saturating_mul(NonZeroUsize::new(4).unwrap())
        }
        Err(err) => {
            error!(
                "Failed to determine number of available CPU threads; defaulting to 1 thread: {err}"
            );
            NonZeroUsize::MIN
        }
    }
}
