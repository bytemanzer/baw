use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context as _, bail};
use bevy::asset::AssetPath;
use bevy::asset::meta::{AssetAction, AssetMeta};
use bevy::reflect::TypePath;
use bevy_alo::models::{AloLoader, AloMeta};
use petro_alo::AloFile;
use petro_alo::mesh::ShaderParam;
use petro_meg::path::{MegPath, MegPathBuf, meg_path};
use tracing::{Level, instrument};

use crate::assets::{Asset, FileSystemView};
use crate::extraction::textures::TextureExtractor;
use crate::processing::{FileWriter, ProcessorContext};

pub struct ModelExtractor<'a, 'p> {
    fs: &'a FileSystemView,
    output: &'p FileWriter<'a>,
    textures: &'p TextureExtractor<'a, 'p>,
    prefix: PathBuf,
    /// Models which have already been found and processed, mapped to the extracted asset path to
    /// use for that model.
    ///
    /// The key used is the filename without any directory. Files are assumed to be in the
    /// Art/Models directory.
    found_models: Mutex<HashMap<MegPathBuf, PathBuf>>,
}

impl<'a, 'p> ModelExtractor<'a, 'p> {
    /// Create a new config extractor that reads from the specified filesystem and writes to the
    /// given file output.
    pub fn new(
        fs: &'a FileSystemView,
        output: &'p FileWriter<'a>,
        textures: &'p TextureExtractor<'a, 'p>,
    ) -> Self {
        Self {
            fs,
            output,
            textures,
            prefix: PathBuf::new(),
            found_models: Default::default(),
        }
    }

    /// Sets the path prefix to use for output files.
    pub fn with_prefix(mut self, prefix: impl Into<PathBuf>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Create an output path from a source model name.
    fn model_name_to_path(&self, name: &MegPath) -> anyhow::Result<PathBuf> {
        let file_name = match name.file_name() {
            None => bail!("model file name is empty"),
            Some(name) => name.to_lowercase().into_string(),
        };
        let mut output_model_file = self.prefix.join("art");
        output_model_file.push("models");
        output_model_file.push(file_name);
        Ok(output_model_file)
    }

    /// If the given model has already been processed, produces the existing output path.
    ///
    /// Return the path along with true if the model was already processed or false if the model
    /// still needs to be processed.
    fn find_or_insert_model_path_for(&self, name: &MegPath) -> anyhow::Result<(PathBuf, bool)> {
        use std::collections::hash_map::Entry;
        if name.is_empty() {
            bail!("model name must be non-empty");
        }
        if let Some(parent) = name.parent()
            && !parent.is_empty()
        {
            bail!("Expected a bare file name, but got {name}");
        }

        let mut found_models = self.found_models.lock().unwrap();
        match found_models.entry(name.to_owned()) {
            Entry::Occupied(entry) => Ok((entry.get().to_owned(), true)),
            Entry::Vacant(entry) => {
                let path = self.model_name_to_path(name)?;
                entry.insert(path.clone());
                Ok((path, false))
            }
        }
    }

    /// Find a model file, dispatching a copy of the model to the output. Returns the path that the
    /// model will have in the output directory, including any relevant prefix.
    ///
    /// The model name should be the bare model name, no directory.
    #[instrument(skip_all, level = Level::DEBUG, fields(model.name = name.as_str()))]
    pub async fn find_model(
        &'p self,
        ctx: &ProcessorContext<'p>,
        name: &MegPath,
    ) -> anyhow::Result<PathBuf> {
        let (output_model_file, already_processed) = self.find_or_insert_model_path_for(name)?;
        if already_processed {
            // The eixsting processing run might fail, but that's OK as long as the error gets
            // reported somewhere. We don't need to force every check for the same model name to
            // always fail.
            return Ok(output_model_file);
        }

        const MODELS: &MegPath = meg_path!("Data\\Art\\Models");
        let path = MODELS.join(name);
        let asset = match self.fs.get(&path) {
            Some(asset) => asset,
            None => bail!("Model {name} not found in {MODELS}"),
        };

        ctx.spawn(self.output.copy_asset(asset, output_model_file.clone()))
            .await;
        ctx.spawn(self.find_model_dependencies(ctx.clone(), asset, output_model_file.clone()))
            .await;

        // TODO: read the model and find needed textures; emit a meta file mapping those textures.
        Ok(output_model_file)
    }

    /// Load the given model asset and find all other assets it depends on. Emit those to the output
    /// directory, along with metadata describing how to map texture from this ALO file to the
    /// output.
    async fn find_model_dependencies(
        &'p self,
        ctx: ProcessorContext<'p>,
        asset: &'a Asset,
        mut output_model_path: PathBuf,
    ) -> anyhow::Result<()> {
        let alo = load_alo(asset)?;
        let mut meta = AloMeta::new();
        for (param_name, texture_name) in shader_texture_params(alo) {
            let texture_name_meg = MegPath::from_bytes(texture_name.as_os_str().as_encoded_bytes())
                .with_context(|| {
                    format!(
                        "Texture name {} from shader param {param_name} was not a valid MegPath",
                        texture_name.display(),
                    )
                })?;
            let path = self.textures.find_texture(&ctx, texture_name_meg).await?;
            // We expect that the textures manager will always return the same path for the same
            // input, so we don't worry about checking for dupes or compatibility.
            meta.texture_mapping
                .insert(texture_name, AssetPath::from_path_buf(path));
        }

        let meta = AssetMeta::<AloLoader, ()>::new(AssetAction::Load {
            loader: AloLoader::type_path().to_string(),
            settings: meta,
        });

        output_model_path.add_extension("meta");
        ctx.spawn(self.output.generate_conf(meta, output_model_path))
            .await;

        Ok(())
    }
}

/// Load the given asset as an [`AloFile`]
fn load_alo(asset: &Asset) -> anyhow::Result<AloFile> {
    let mut bytes = Vec::new();
    asset
        .open_reader()
        .with_context(|| format!("Failed to open reader for ALO asset {asset:?}"))?
        .read_to_end(&mut bytes)
        .with_context(|| format!("Failed to read contents of ALO asset {asset:?}"))?;
    AloFile::parse(&bytes).with_context(|| format!("Failed to parse ALO asset {asset:?}"))
}

/// Gets an iterator over all of the [`Texture`][ShaderParam::Texture] shader params in the given
/// ALO file.
fn shader_texture_params(alo: AloFile) -> impl Iterator<Item = (String, PathBuf)> {
    alo.meshes
        .into_iter()
        .flat_map(|mesh| mesh.submeshes)
        .flat_map(|submesh| submesh.shader.params)
        .filter_map(|(name, param)| match param {
            ShaderParam::Texture(path) => Some((name, path)),
            _ => None,
        })
}
