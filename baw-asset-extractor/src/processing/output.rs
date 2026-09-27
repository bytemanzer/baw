use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use anyhow::{Context as _, bail};
use petro_meg::path::MegPath;
use tracing::info;

use crate::assets::Asset;

/// How a destination path was written.
enum WriteType<'a> {
    AssetCopy { source: &'a MegPath },
}

impl<'a> WriteType<'a> {
    /// Checks if an existing write type is compatible with this one.
    fn check_compatibility(&self, existing: &WriteType<'_>, dest: &Path) -> anyhow::Result<()> {
        match (self, existing) {
            (WriteType::AssetCopy { source }, WriteType::AssetCopy { source: existing }) => {
                if source != existing {
                    bail!(
                        "Cannot copy {source} to {}, because it was already copied from {existing}",
                        dest.display()
                    );
                }
            }
        }
        Ok(())
    }
}

/// Current state of an output file, which determines if an action
#[derive(Default)]
enum ExistingFileState {
    /// We have not checked for an existing file yet.
    #[default]
    NotChecked,
    /// There was not an existing output file, so it should be written.
    Missing,
    /// There was an existing output file, but it was older than the source, so it should be
    /// written.
    OlderThanSource,
    /// There was an existing output file, but it was older than the generator that produces it, so
    /// it should be written.
    OlderThanGenerator,
    /// There was an existing output file, and it was newer than the source and/or generator, so it
    /// should not be written.
    UpToDate,
}

/// Records information about an output file.
struct OutputFile<'a> {
    /// Which type of write created the file on this run.
    write_type: WriteType<'a>,
    /// Records the state that the output file was in the first time it was handled by the
    /// [FileWriter]. None if the output state hasn't been determined yet.
    existing_file: ExistingFileState,
}

impl<'a> OutputFile<'a> {
    /// Create an OutputFile writer.
    fn new(write_type: WriteType<'a>) -> Self {
        Self {
            write_type,
            existing_file: ExistingFileState::NotChecked,
        }
    }
}

/// Configures how the Output system checks for existing files.
#[derive(Debug, PartialEq, Eq)]
pub enum ExistingFileHandling {
    /// Do not check for existing files. Write all files to output even if they already exist.
    WriteAll,
    /// If the outpuf file is newer than the source, skip writing it.
    SkipIfNewer,
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
    /// How existing files are handled.
    existing_file_handling: ExistingFileHandling,
    /// Modification time of the asset processor system.
    ///
    /// Certain asset types, such as those extracted from configs, may vary depending on when the
    /// generator was modified, not just when the source was modified. For those types of files,
    /// they will be written if the existing file is older than this typestamp, not just if the
    /// source is newer than the existing file.
    generator_mtime: Option<SystemTime>,
}

impl WriterConfig {
    /// Create a new writer config with the given output directory and default settings.
    pub fn new(base: PathBuf) -> Self {
        Self {
            base,
            prefix: PathBuf::new(),
            existing_file_handling: ExistingFileHandling::SkipIfNewer,
            generator_mtime: None,
        }
    }

    /// Set the prefix to require on all written output paths.
    pub fn set_prefix(mut self, prefix: impl Into<PathBuf>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Set how existing files are to be handled.
    pub fn set_existing_file_handling(
        mut self,
        existing_file_handling: ExistingFileHandling,
    ) -> Self {
        self.existing_file_handling = existing_file_handling;
        self
    }

    /// Sets the modification time for the file extractor.
    pub fn set_generator_mtime(mut self, time: SystemTime) -> Self {
        self.generator_mtime = Some(time);
        self
    }
}

/// Manages outputting assets to a destination folder.
pub struct FileWriter<'a> {
    /// Configuration for the FileWriter.
    config: &'a WriterConfig,
    /// Set of output paths, relative to the base, which have already been written.
    outputs: Mutex<HashMap<PathBuf, OutputFile<'a>>>,
}

impl<'a> FileWriter<'a> {
    /// Create a new output that writes files to the given base directory.
    pub fn new(config: &'a WriterConfig) -> Self {
        Self {
            config,
            outputs: Default::default(),
        }
    }

    /// Insert a write marker of the speified `write_type` for the given `dest` [`Path`].
    ///
    /// If another write to the same path was already requested, checks for compatiblity and
    /// produces an error if the two writes are not compatible.
    ///
    /// Returns true if the value was newly inserted. If the copy already exists in the outputs,
    /// returns false. False means that another task is already handling copying that asset to that
    /// path.
    fn insert_write(&self, write_type: WriteType<'a>, dest: &Path) -> anyhow::Result<bool> {
        if !dest.starts_with(&self.config.prefix) {
            bail!(
                "Destination path {} must start with {}",
                dest.display(),
                self.config.prefix.display()
            );
        }
        let mut outputs = self.outputs.lock().unwrap();
        Ok(match outputs.entry(dest.to_path_buf()) {
            Entry::Occupied(entry) => {
                let existing = entry.get();
                write_type.check_compatibility(&existing.write_type, entry.key())?;
                false
            }
            Entry::Vacant(entry) => {
                entry.insert(OutputFile::new(write_type));
                true
            }
        })
    }

    /// Updates the existing file state for a file. Panics if the file is not found.
    fn update_existing_file_state(&self, path: &Path, state: ExistingFileState) {
        let mut outputs = self.outputs.lock().unwrap();
        outputs
            .get_mut(path)
            .expect("path must alread be inserted before calling update_existing_file_state")
            .existing_file = state;
    }

    /// If existing output checks are enabled, checks if the destination file exists, and checks
    /// whether it should be overwritten given the source. Returns true if the existing output can
    /// be used, or false if the existing file should be overwritten.
    ///
    /// Can produce an error if there are issues checking asset modification times.
    fn can_reuse_existing_output<S: Source + ?Sized>(
        &self,
        source: &S,
        dest: &Path,
    ) -> anyhow::Result<bool> {
        if self.config.existing_file_handling == ExistingFileHandling::WriteAll {
            return Ok(false);
        }
        let existing_mtime = match fs::metadata(dest) {
            Ok(meta) => meta.modified().with_context(|| {
                format!("while getting the modification time for {}", dest.display())
            })?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                self.update_existing_file_state(dest, ExistingFileState::Missing);
                return Ok(false);
            }
            Err(err) => Err(err).context(format!(
                "while checking for an existing file at {} to check modification time",
                dest.display()
            ))?,
        };
        let source_mtime = source.mtime().with_context(|| {
            format!(
                "while getting the modification for the source of the write to {}",
                dest.display()
            )
        })?;
        if existing_mtime <= source_mtime {
            self.update_existing_file_state(dest, ExistingFileState::OlderThanSource);
            return Ok(false);
        }
        if source.depends_on_generator()
            && let Some(gen_mtime) = self.config.generator_mtime
            && existing_mtime <= gen_mtime
        {
            self.update_existing_file_state(dest, ExistingFileState::OlderThanGenerator);
            return Ok(false);
        }
        self.update_existing_file_state(dest, ExistingFileState::UpToDate);
        Ok(true)
    }

    /// Copy the given asset to the specified destination path.
    pub async fn copy_asset(&self, asset: &'a Asset, dest: PathBuf) -> anyhow::Result<()> {
        let write_type = WriteType::AssetCopy {
            source: asset.path(),
        };
        if !self.insert_write(write_type, &dest)? {
            return Ok(());
        }
        if self.can_reuse_existing_output(asset, &dest)? {
            return Ok(());
        }
        let full_dest = self.config.base.join(&dest);
        if let Some(dir) = full_dest.parent() {
            fs::create_dir_all(dir).with_context(|| {
                format!(
                    "failed to create the parent directories of for output file {}",
                    dest.display()
                )
            })?;
        }
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
}

/// Source of an output. This trait is used to determine whether the output needs to be re-written.
trait Source {
    /// Get the relevant modification time for this source.
    fn mtime(&self) -> io::Result<SystemTime>;

    /// Whether an output from this source depends on the asset extractor. Direct copies generally
    /// don't require updating if the generator has chaned, while derived assets might, since the
    /// generator could emit a different value after it has changed.
    fn depends_on_generator(&self) -> bool;
}

impl Source for Asset {
    fn mtime(&self) -> io::Result<SystemTime> {
        self.modified_time()
    }

    fn depends_on_generator(&self) -> bool {
        false
    }
}
