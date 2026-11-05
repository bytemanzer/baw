use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use ain::{AinStr, AinString};
use anyhow::{Context as _, bail};
use petro_meg::path::{MegPath, meg_path};
use tracing::{Level, instrument};

use crate::assets::{Asset, FileSystemView};
use crate::processing::{FileWriter, ProcessorContext};

pub struct TextureExtractor<'a> {
    fs: &'a FileSystemView,
    output: &'a FileWriter<'a>,
    prefix: PathBuf,
    /// Textures which have been extracted. The key used is the bare name with the extension removed
    /// (file_stem, so only one prefix removed if there are multiple). It should not include a
    /// directory, textures are assumed to live in the Art/Textures directory.
    found_textures: Mutex<HashMap<AinString, PathBuf>>,
}

impl<'a> TextureExtractor<'a> {
    /// Create a new config extractor that reads from the specified filesystem and writes to the
    /// given file output.
    pub fn new(fs: &'a FileSystemView, output: &'a FileWriter<'a>) -> Self {
        Self {
            fs,
            output,
            prefix: PathBuf::new(),
            found_textures: Default::default(),
        }
    }

    /// Sets the path prefix to use for output files.
    pub fn with_prefix(mut self, prefix: impl Into<PathBuf>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Finds an asset matching the given texture's file stem, ignoring extension, within the
    /// texture directory.
    fn find_matching_texture(&self, stem: &AinStr) -> anyhow::Result<&Asset> {
        const TEXTURES: &MegPath = meg_path!("Data\\Art\\Textures");

        let stem_path = MegPath::from_ain_str(stem)
            .with_context(|| format!("converting the file name {stem} back to a MegPath"))?;
        let without_ext = TEXTURES.join(stem_path);

        for file in self.fs.asset_range(without_ext..) {
            if file.path().file_stem() == Some(stem) {
                return Ok(file);
            }
            // If we're past the point where file stems start with the same prefix we're looking
            // for, we've gone too far.
            if let Some(name) = file.path().file_name()
                && !name.starts_with(stem)
            {
                break;
            }
            // Likewise, if we've left the textures directory, we've gone too far.
            if let Some(parent) = file.path().parent()
                && parent != TEXTURES
            {
                break;
            }
        }
        bail!("No texture was found with name {stem}");
    }

    /// Convert the found texture file path to a path in the output directory with the prefix.
    fn texture_path_to_output_path(&self, mapped_file: &MegPath) -> anyhow::Result<PathBuf> {
        let file_name = match mapped_file.file_name() {
            None => bail!("mapped file name is empty"),
            Some(name) => name.to_lowercase().into_string(),
        };
        let mut output_texture_file = self.prefix.join("textures");
        output_texture_file.push(file_name);
        Ok(output_texture_file)
    }

    /// If the given texture has already been processed, produces the existing output path.
    ///
    /// Return the path along with true if the texture was already processed or false if the texture
    /// still needs to be processed.
    ///
    /// `stem` is the stem of the original file name, which the texture tracker is keyed off of.
    /// `usage` is used to check for conflicting usages with different texture settings.
    /// `mapped_file` is the file found in `self.fs` which this texture maps to.
    fn find_or_insert_texture_path_for(
        &self,
        stem: AinString,
        mapped_file: &MegPath,
    ) -> anyhow::Result<(PathBuf, bool)> {
        use std::collections::hash_map::Entry;

        let mut found_textures = self.found_textures.lock().unwrap();
        match found_textures.entry(stem) {
            Entry::Occupied(entry) => Ok((entry.get().clone(), true)),
            Entry::Vacant(entry) => {
                let output_path = self.texture_path_to_output_path(mapped_file)?;
                entry.insert(output_path.clone());
                Ok((output_path, false))
            }
        }
    }

    /// Find a texture file, dispatching a copy of the texture to the output. Returns the path that
    /// the model will have in the output directory, including any relevant prefix.
    ///
    /// The model name should be the bare model name, no directory.
    #[instrument(skip_all, level = Level::DEBUG, fields(model.name = name.as_str()))]
    pub async fn find_texture(
        &'a self,
        ctx: &ProcessorContext<'a>,
        name: &MegPath,
    ) -> anyhow::Result<PathBuf> {
        let stem = validate_name_and_get_stem(name)?;
        // This part can run more than once since we haven't done a lock and dedupe yet, but that's
        // fine since find_matching_texture is just a simple search.
        let mapped_file = self.find_matching_texture(&stem)?;
        let (output_texture_file, already_processed) =
            self.find_or_insert_texture_path_for(stem, mapped_file.path())?;
        if already_processed {
            // The eixsting processing run might fail, but that's OK as long as the error gets
            // reported somewhere. We don't need to force every check for the same model name to
            // always fail.
            return Ok(output_texture_file);
        }

        ctx.spawn(
            self.output
                .copy_asset(mapped_file, output_texture_file.clone()),
        )
        .await;

        Ok(output_texture_file)
    }
}

/// Validate the given texture name, and get the stem, stripping extensions.
fn validate_name_and_get_stem(name: &MegPath) -> anyhow::Result<AinString> {
    if name.is_empty() {
        bail!("texture name must be non-empty");
    }
    if let Some(parent) = name.parent()
        && !parent.is_empty()
    {
        bail!("Expected a bare file name, but got {name}");
    }
    let stem = match name.file_stem() {
        Some(stem) => stem.to_ain_string(),
        None => bail!("texture is missing a file-name"),
    };
    Ok(stem)
}
