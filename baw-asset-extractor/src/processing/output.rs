use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context as _, bail};
use serde::Serialize;
use tracing::{Level, debug, instrument};

use crate::assets::Asset;

/// A brief summary of how an asset was written.
pub enum WriteType<'a> {
    /// Write the given asset to the output path.
    Asset { original: &'a Asset },
    /// Write a generated config file of the specified type.
    GenConf { config_type: &'static str },
}

/// State of the existing file for a write.
pub enum ReuseExistingDecision {
    /// No re-use decision has been made yet.
    Undecided,
    /// File must be written because *all* output files are being written.
    WriteAll,
    /// File must be written because it was generated and we don't currently support re-use for
    /// generated files.
    Generated,
    /// File must be written because no existing file was found, so there's nothing to re-use.
    NotFound,
    /// File must be written because the existing file was older than the source.
    OlderThanSource,
    /// File can be skipped because it is already newer than the source.
    NewerThanSource,
}

/// Tracks metadata about an output file being written.
pub struct OutputFile<'a> {
    /// How this output file was written.
    pub write_type: WriteType<'a>,
    /// Whether we decided to re-use an existing output file, and why.
    pub reuse_existing_decision: ReuseExistingDecision,
}

impl<'a> OutputFile<'a> {
    /// Create a new output file entry for the given write type.
    fn new(write_type: WriteType<'a>) -> Self {
        let reuse_existing_decision = match write_type {
            WriteType::GenConf { .. } => ReuseExistingDecision::Generated,
            _ => ReuseExistingDecision::Undecided,
        };
        Self {
            write_type,
            reuse_existing_decision,
        }
    }
}

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
    config: WriterConfig,
    /// Set of output paths, relative to the base, which have already been written.
    outputs: Mutex<HashMap<PathBuf, OutputFile<'a>>>,
}

impl<'a> FileWriter<'a> {
    /// Create a new output that writes files to the given base directory.
    pub fn new(config: WriterConfig) -> Self {
        Self {
            config,
            outputs: Default::default(),
        }
    }

    /// Consumes this output and produces an iterator over the output items.
    pub fn into_outputs(self) -> impl Iterator<Item = (PathBuf, OutputFile<'a>)> {
        self.outputs.into_inner().unwrap().into_iter()
    }

    /// Insert a write marker for the specified path.
    ///
    /// Returns an error if the specified path was already written this session.
    fn insert_write(&self, dest: &Path, write_type: WriteType<'a>) -> anyhow::Result<()> {
        if !dest.starts_with(&self.config.prefix) {
            bail!(
                "Destination path {} must start with {}",
                dest.display(),
                self.config.prefix.display()
            );
        }
        let mut outputs = self.outputs.lock().unwrap();
        if outputs
            .insert(dest.to_path_buf(), OutputFile::new(write_type))
            .is_some()
        {
            bail!("Output file {} was already written", dest.display());
        } else {
            Ok(())
        }
    }

    /// Update the file-re-use decision for the given file.
    fn update_reuse_decision(&self, dest: &Path, decision: ReuseExistingDecision) {
        let mut outputs = self.outputs.lock().unwrap();
        let output = outputs
            .get_mut(dest)
            .expect("tried to update the output path for a path that is not yet set as an output");
        output.reuse_existing_decision = decision;
    }

    /// If existing output checks are enabled, checks if the destination file exists, and checks
    /// whether it should be overwritten given the source. Returns true if the existing output can
    /// be used, or false if the existing file should be overwritten.
    ///
    /// Can produce an error if there are issues checking asset modification times.
    fn can_reuse_existing_output(&self, asset: &Asset, dest: &Path) -> anyhow::Result<bool> {
        if self.config.write_all {
            self.update_reuse_decision(dest, ReuseExistingDecision::WriteAll);
            return Ok(false);
        }
        let full_dest = self.config.base.join(&dest);
        let existing_mtime = match fs::metadata(full_dest) {
            Ok(meta) => meta.modified().with_context(|| {
                format!("Failed to get the modification time for {}", dest.display())
            })?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                self.update_reuse_decision(dest, ReuseExistingDecision::NotFound);
                return Ok(false);
            }
            Err(err) => Err(err).context(format!(
                "Failed checking for an existing file at {} to check modification time",
                dest.display()
            ))?,
        };
        let source_mtime = asset.modified_time().with_context(|| {
            format!(
                "failed to get the modification for the source of the write to {}",
                dest.display()
            )
        })?;
        if source_mtime < existing_mtime {
            self.update_reuse_decision(dest, ReuseExistingDecision::NewerThanSource);
            Ok(true)
        } else {
            self.update_reuse_decision(dest, ReuseExistingDecision::OlderThanSource);
            Ok(false)
        }
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
    #[instrument(err(Debug), skip_all, level = Level::DEBUG, fields(asset = asset.path().as_str(), dest))]
    pub async fn copy_asset(&self, asset: &'a Asset, dest: PathBuf) -> anyhow::Result<()> {
        self.insert_write(&dest, WriteType::Asset { original: asset })?;
        if self.config.dry_run {
            if self.can_reuse_existing_output(asset, &dest)? {
                debug!(
                    "Would re-use existing file {} for {}",
                    dest.display(),
                    asset.path()
                );
            } else {
                debug!("Would write {} from {}", dest.display(), asset.path());
            }
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
        debug!("Copied asset {asset:?} to {}", full_dest.display());
        Ok(())
    }

    /// Output some generated configuration as Ron.
    #[instrument(err(Debug), skip_all, level = Level::DEBUG, fields(asset = std::any::type_name::<T>(), dest))]
    pub async fn generate_conf<T: Serialize>(&self, conf: T, dest: PathBuf) -> anyhow::Result<()> {
        let write_type = WriteType::GenConf {
            config_type: std::any::type_name::<T>(),
        };
        self.insert_write(&dest, write_type)?;
        if self.config.dry_run {
            debug!(
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
        debug!(
            "Created {} from a config of type {}",
            dest.display(),
            std::any::type_name::<T>()
        );

        let pretty = ron::ser::PrettyConfig::new()
            .new_line("\n")
            .compact_ranges(true)
            .struct_names(false);
        ron::Options::default()
            .to_io_writer_pretty(output, &conf, pretty)
            .with_context(|| format!("Failed to write serialized config to {}", dest.display()))?;
        Ok(())
    }
}
