use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, bail};
use baw_game_object::scene::{GameObjectScene, ModelAsset};
use heck::ToSnakeCase;
use petro_meg::path::{MegPath, meg_path};
use smol::stream::{self, StreamExt as _};
use tracing::{Level, debug, instrument, trace};

use crate::assets::{Asset, FileSystemView};
use crate::extraction::models::ModelExtractor;
use crate::extraction::xmlmodel::{FileListXml, GameObjectListXml, GameObjectXml, apply_parents};
use crate::processing::{FileWriter, ProcessorContext};
use crate::xml::{self, TaggedXml};

/// Extracts the game's configuration.
pub struct ConfigExtractor<'a, 'p> {
    /// Filesystem view to look up config files in.
    fs: &'a FileSystemView,
    /// Model extractor to use to pull models.
    models: &'p ModelExtractor<'a, 'p>,
    /// Output to write generated configs to.
    output: &'p FileWriter<'a>,
    /// Path prefix to prepend to any file names emitted.
    prefix: PathBuf,
}

impl<'a, 'p> ConfigExtractor<'a, 'p> {
    /// Create a new config extractor that reads from the specified filesystem and writes to the
    /// given file output.
    pub fn new(
        fs: &'a FileSystemView,
        models: &'p ModelExtractor<'a, 'p>,
        output: &'p FileWriter<'a>,
    ) -> Self {
        Self {
            fs,
            models,
            output,
            prefix: PathBuf::new(),
        }
    }

    /// Sets the path prefix to use for output files.
    pub fn with_prefix(mut self, prefix: impl Into<PathBuf>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Find the data files in the sources and extract the bits we care about.
    #[instrument(err(Debug), skip_all, level = Level::DEBUG)]
    pub async fn extract_configs(&'p self, ctx: ProcessorContext<'p>) -> anyhow::Result<()> {
        ctx.spawn(self.extract_game_objects(ctx.clone())).await;
        Ok(())
    }

    /// Find all game objects declared in game object data files and extract them as configs.
    #[instrument(err(Debug), skip_all, level = Level::DEBUG)]
    async fn extract_game_objects(&'p self, ctx: ProcessorContext<'p>) -> anyhow::Result<()> {
        const GAME_OBJECT_FILES: &MegPath = meg_path!("Data\\XML\\GameObjectFiles.XML");
        let objectfiles = match self.fs.get(GAME_OBJECT_FILES) {
            Some(asset) => asset,
            None => bail!("{GAME_OBJECT_FILES} not found"),
        };
        let objectfiles = objectfiles
            .open_reader()
            .with_context(|| format!("failed to open {objectfiles:?}"))?;
        let objectfiles: FileListXml = xml::deserialize_xml_utf8_or_cp1252(objectfiles)?;

        let gameobjects = self.parse_game_object_list_files(&ctx, objectfiles).await;
        self.create_game_object_configs(&ctx, gameobjects).await?;

        Ok(())
    }

    /// Takes an iterator over unprefixed XML filenames and returns an iterator over ones that exist
    /// on the filesystem. Files that don't exist are skipped.
    fn find_unprefixed_xml_files<I, P>(&self, filenames: I) -> impl Iterator<Item = &'a Asset>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<MegPath>,
    {
        filenames
            .into_iter()
            .filter_map(|name| self.find_unprefixed_xml_file(name.as_ref()))
    }

    /// If the given filename refers to an XML file in `Data\XML`, finds the asset for that XML
    /// file.
    fn find_unprefixed_xml_file(&self, filename: &MegPath) -> Option<&'a Asset> {
        const XML_DIR: &MegPath = meg_path!("Data\\XML");
        let fullname = XML_DIR.join(filename);
        match self.fs.get(fullname) {
            Some(asset) => Some(asset),
            None => {
                debug!("XML file {filename} not found in {XML_DIR}");
                None
            }
        }
    }

    /// Finds all game object files in the given file list, then loads and parses them, and returns
    /// the combined complete [`GameObjectListXml`].
    fn parse_game_object_list_files(
        &'p self,
        ctx: &ProcessorContext<'p>,
        objectfiles: FileListXml,
    ) -> impl Future<Output = Vec<GameObjectXml>> {
        stream::iter(self.find_unprefixed_xml_files(objectfiles.files))
            .flat_map(move |asset| {
                stream::once_future(ctx.spawn_wait(self.parse_game_object_list_file(asset)))
            })
            // Ignore task failures, since the executor will shut down on error anyway.
            .filter_map(Result::ok)
            .flat_map(|vec| stream::iter(vec))
            .collect()
    }

    /// Visit a Data XML file, parsing and returning it if present. If the data file isn't present
    /// in the XML directory, returns an empty game object list.
    #[instrument(err(Debug), skip_all, fields(asset = asset.path().as_str()))]
    async fn parse_game_object_list_file(
        &'p self,
        asset: &'a Asset,
    ) -> anyhow::Result<Vec<GameObjectXml>> {
        trace!("Parsing {}", asset.path());
        let gameobjects = asset
            .open_reader()
            .with_context(|| format!("failed to open {asset:?}"))?;
        let gameobjects: TaggedXml<GameObjectListXml> =
            xml::deserialize_xml_utf8_or_cp1252(gameobjects)
                .with_context(|| format!("failed to parse {asset:?}"))?;
        let collection: Arc<str> = gameobjects.tag.into();
        let fill_collection_kind = move |tagged: TaggedXml<GameObjectXml>| {
            let mut value = tagged.value;
            value.collection = collection.clone();
            value.kind = tagged.tag;
            value
        };
        let gameobjects = gameobjects
            .value
            .game_objects
            .into_iter()
            .map(fill_collection_kind)
            .collect();
        Ok(gameobjects)
    }

    /// Applies parents and then spawns config writers for every geme object type.
    async fn create_game_object_configs(
        &'p self,
        ctx: &ProcessorContext<'p>,
        mut gameobjects: Vec<GameObjectXml>,
    ) -> anyhow::Result<()> {
        apply_parents(&mut gameobjects)?;
        for object in gameobjects {
            ctx.spawn(self.create_game_object_config(ctx.clone(), object))
                .await;
        }
        Ok(())
    }

    /// Visit a ship config, emitting a config file and
    #[instrument(err(Debug), skip_all, fields(object.name = object.name.as_str()))]
    async fn create_game_object_config(
        &'p self,
        ctx: ProcessorContext<'p>,
        object: GameObjectXml,
    ) -> anyhow::Result<()> {
        let model = match object.space_model_name.0 {
            None => {
                debug!("Game Object {} has no model", object.name);
                None
            }
            Some(ref model) if model.is_empty() => {
                debug!("Model name for Game Object {} is empty", object.name);
                None
            }
            Some(ref model) => {
                let model = self.models.find_model(&ctx, model).await?;
                Some(ModelAsset {
                    model: model.into(),
                    scale: object.scale_factor.unwrap_or(1.0),
                })
            }
        };
        let dest = self.file_path_for_game_object(&object)?;

        let config = GameObjectScene {
            name: object.name.into(),
            model,
        };

        ctx.spawn(self.output.generate_conf(config, dest)).await;

        Ok(())
    }

    /// Chooses a file path for a game object.
    fn file_path_for_game_object(&self, object: &GameObjectXml) -> anyhow::Result<PathBuf> {
        let collection = object.collection.to_snake_case();
        let kind = object.kind.to_snake_case();
        let name = object.name.to_ascii_lowercase();

        check_path_safe(&collection, "collection")?;
        check_path_safe(&kind, "kind")?;
        check_path_safe(&name, "name")?;

        let mut path = self.prefix.to_path_buf();
        path.push("config");
        path.push(collection);
        path.push(kind);
        path.push(name);
        path.add_extension("gos");
        Ok(path)
    }
}

/// Checks that the given path contains a single, non-empty normal path component.
///
/// This prevents accidental directory traversals.
fn check_path_safe(path: impl AsRef<Path>, desc: &str) -> anyhow::Result<()> {
    let path = path.as_ref();
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(component)), None)
            if !component.is_empty() && component == path.as_os_str() =>
        {
            // If this is a normal component, non-empty, and equal to the full path, we are good.
            // Checking that the component is equal to the path checks for trailing slashes in the
            // name.
            Ok(())
        }
        _ => bail!("invalid game object {desc}: {}", path.display()),
    }
}
