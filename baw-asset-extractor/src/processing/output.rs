use std::collections::HashSet;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context as _, bail};
use serde::Serialize;
use tracing::info;

use crate::assets::Asset;

/// Configures how outputs are to be written.
pub struct WriterConfig {
    /// Base path to write output to. This is the path to the assets directory, it does not include
    /// the extraction-specific prefix.
    base: PathBuf,
    /// A prefix which must be included in every output path.
    ///
    /// While `base` is the absolute portion of the destination directory, which is not included in
    /// paths used by the game engine, the `prefix` is the relative potion, which assets must use to
    /// reference each other.
    ///
    /// Since assets emitted need to include this prefix in cross-references, we don't automatically
    /// prepend it to output file names. The theory is that any extractor which is mistakenly
    /// missing the prefix in cross-references is likely to also miss it when emitting outputs, so
    /// making it an error to miss the prefix when emitting an output gives us a chance to catch
    /// such mistakes.
    prefix: PathBuf,
    /// If true, write all output files without checking for existing outputs that can be re-used.
    write_all: bool,
    /// If true, don't actually write any outputs to the destination dir.
    dry_run: bool,
}

impl WriterConfig {
    /// Create a new writer config with the given output directory and default settings.
    pub fn new(base: PathBuf) -> Self {
        Self {
            base,
            prefix: PathBuf::new(),
            write_all: false,
            dry_run: false,
        }
    }

    /// Set the prefix to require on all written output paths.
    pub fn set_prefix(mut self, prefix: impl Into<PathBuf>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Set whether to write all output files unconditionally, rather than checking for existing
    /// files to dedup.
    pub fn set_write_all(mut self, write_all: bool) -> Self {
        self.write_all = write_all;
        self
    }

    /// Set whether to do a dry run; i.e. not acutally copy output.
    pub fn set_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }
}

/// Manages outputting assets to a destination folder.
pub struct FileWriter<'a> {
    /// Configuration for the FileWriter.
    config: &'a WriterConfig,
    /// Set of output paths, relative to the base, which have already been written.
    outputs: Mutex<HashSet<PathBuf>>,
}

impl<'a> FileWriter<'a> {
    /// Create a new output that writes files to the given base directory.
    pub fn new(config: &'a WriterConfig) -> Self {
        Self {
            config,
            outputs: Default::default(),
        }
    }

    /// Insert a write marker for the specified path.
    ///
    /// Returns an error if the specified path was already written this session.
    fn insert_write(&self, dest: &Path) -> anyhow::Result<()> {
        if !dest.starts_with(&self.config.prefix) {
            bail!(
                "Destination path {} must start with {}",
                dest.display(),
                self.config.prefix.display()
            );
        }
        let mut outputs = self.outputs.lock().unwrap();
        if outputs.insert(dest.to_path_buf()) {
            Ok(())
        } else {
            bail!("Output file {} was already written", dest.display());
        }
    }

    /// If existing output checks are enabled, checks if the destination file exists, and checks
    /// whether it should be overwritten given the source. Returns true if the existing output can
    /// be used, or false if the existing file should be overwritten.
    ///
    /// Can produce an error if there are issues checking asset modification times.
    fn can_reuse_existing_output(&self, asset: &Asset, dest: &Path) -> anyhow::Result<bool> {
        if self.config.write_all {
            return Ok(false);
        }
        let existing_mtime = match fs::metadata(dest) {
            Ok(meta) => meta.modified().with_context(|| {
                format!("while getting the modification time for {}", dest.display())
            })?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(false);
            }
            Err(err) => Err(err).context(format!(
                "while checking for an existing file at {} to check modification time",
                dest.display()
            ))?,
        };
        let source_mtime = asset.modified_time().with_context(|| {
            format!(
                "while getting the modification for the source of the write to {}",
                dest.display()
            )
        })?;
        Ok(source_mtime < existing_mtime)
    }

    /// Prepends the base path to the dest dir and ensures the directory exists by creating all
    /// intermediate directories on the path to the file. Does not create the actual output file.
    fn create_dest_dir(&self, dest: &Path) -> anyhow::Result<PathBuf> {
        let full_dest = self.config.base.join(&dest);
        if let Some(dir) = full_dest.parent() {
            fs::create_dir_all(dir).with_context(|| {
                format!(
                    "failed to create the parent directories of for output file {}",
                    dest.display()
                )
            })?;
        }
        Ok(full_dest)
    }

    /// Copy the given asset to the specified destination path.
    pub async fn copy_asset(&self, asset: &'a Asset, dest: PathBuf) -> anyhow::Result<()> {
        self.insert_write(&dest)?;
        if self.config.dry_run {
            info!("Would write {} from {}", dest.display(), asset.path());
            return Ok(());
        }
        if self.can_reuse_existing_output(asset, &dest)? {
            return Ok(());
        }
        let full_dest = self.create_dest_dir(&dest)?;
        if let Some(real_path) = asset.real_path() {
            fs::copy(real_path, &full_dest).with_context(|| {
                format!(
                    "failed to copy source file {} to {}",
                    real_path.display(),
                    dest.display()
                )
            })?;
        } else {
            let mut source = asset.open_reader().with_context(|| {
                format!(
                    "failed to open the source asset {} to write to {}",
                    asset.path(),
                    dest.display()
                )
            })?;
            let mut output = File::create(&full_dest).with_context(|| {
                format!(
                    "failed to open the dest file {} for asset {}",
                    dest.display(),
                    asset.path()
                )
            })?;
            io::copy(&mut source, &mut output).with_context(|| {
                format!(
                    "failed to copy from asset {} to {}",
                    asset.path(),
                    dest.display()
                )
            })?;
        }
        info!("Copied asset {asset:?} to {}", full_dest.display());
        Ok(())
    }

    /// Output some generated configuration as Ron.
    pub async fn generate_conf<T: Serialize>(&self, conf: T, dest: PathBuf) -> anyhow::Result<()> {
        self.insert_write(&dest)?;
        if self.config.dry_run {
            info!(
                "Would write {} from a config of type {}",
                dest.display(),
                std::any::type_name::<T>()
            );
            return Ok(());
        }

        // We currently don't check for file re-use for generated files.
        let full_dest = self.create_dest_dir(&dest)?;

        let output = File::create(&full_dest).with_context(|| {
            format!(
                "failed to open the dest file {} for generated config",
                dest.display()
            )
        })?;

        let pretty = ron::ser::PrettyConfig::new()
            .new_line("\n")
            .compact_ranges(true)
            .struct_names(true);
        ron::Options::default()
            .to_io_writer_pretty(output, &conf, pretty)
            .with_context(|| format!("Failed to write serialized config to {}", dest.display()))?;
        Ok(())
    }
}
