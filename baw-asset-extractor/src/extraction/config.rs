use std::path::PathBuf;

use anyhow::{Context as _, bail};
use baw_config::units::ships::ShipConfig;
use petro_meg::path::{MegPath, meg_path};
use smol::stream::{self, StreamExt as _};
use tracing::{Level, debug, instrument, trace, warn};

use crate::assets::{Asset, FileSystemView};
use crate::extraction::models::ModelExtractor;
use crate::extraction::xmlmodel::{FileListXml, GameObjectListXml, SpaceUnitXml};
use crate::processing::{FileWriter, ProcessorContext};
use crate::xml;

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
    #[instrument(err, skip_all, level = Level::DEBUG)]
    pub async fn extract_data_files(&'p self, ctx: ProcessorContext<'p>) -> anyhow::Result<()> {
        const GAME_OBJECT_FILES: &MegPath = meg_path!("Data\\XML\\GameObjectFiles.XML");
        let objectfiles = match self.fs.get(GAME_OBJECT_FILES) {
            Some(asset) => asset,
            None => bail!("{GAME_OBJECT_FILES} not found"),
        };
        let objectfiles = objectfiles
            .open_reader()
            .with_context(|| format!("failed to open {objectfiles:?}"))?;
        let objectfiles: FileListXml = xml::deserialize_xml_utf8_or_cp1252(objectfiles)?;

        let all_game_objects = self.parse_game_object_list_files(ctx.clone(), objectfiles);
        ctx.spawn(self.extract_game_objects(ctx.clone(), all_game_objects))
            .await;

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
        ctx: ProcessorContext<'p>,
        objectfiles: FileListXml,
    ) -> impl Future<Output = GameObjectListXml> {
        stream::iter(self.find_unprefixed_xml_files(objectfiles.files))
            .flat_map(move |asset| {
                stream::once_future(ctx.spawn_wait(self.parse_game_object_list_file(asset)))
            })
            // Ignore task failures, since the executor will shut down on error anyway.
            .filter_map(Result::ok)
            .collect()
    }

    /// Visit a Data XML file, parsing and returning it if present. If the data file isn't present
    /// in the XML directory, returns an empty game object list.
    #[instrument(err, skip_all, fields(asset = asset.path().as_str()))]
    async fn parse_game_object_list_file(
        &'p self,
        asset: &'a Asset,
    ) -> anyhow::Result<GameObjectListXml> {
        trace!("Parsing {}", asset.path());
        let gameobjects = asset
            .open_reader()
            .with_context(|| format!("failed to open {asset:?}"))?;
        let gameobjects: GameObjectListXml = xml::deserialize_xml_utf8_or_cp1252(gameobjects)
            .with_context(|| format!("failed to parse {asset:?}"))?;
        Ok(gameobjects)
    }

    /// Extracts all game objects from the combined gameobject list xml.
    #[instrument(err, skip_all, level = Level::DEBUG)]
    async fn extract_game_objects<F>(
        &'p self,
        ctx: ProcessorContext<'p>,
        all_game_objects: F,
    ) -> anyhow::Result<()>
    where
        F: Future<Output = GameObjectListXml>,
    {
        let mut all_game_objects = all_game_objects.await;
        all_game_objects.apply_all_parents()?;
        self.create_ship_configs(&ctx, all_game_objects.spaceunits)
            .await;
        Ok(())
    }

    /// Go through all ship configs and launch tasks to emit the corresponding configuration files.
    ///
    /// Variant of existing must already have been applied.
    async fn create_ship_configs(
        &'p self,
        ctx: &ProcessorContext<'p>,
        spaceunits: impl IntoIterator<Item = SpaceUnitXml>,
    ) {
        for ship in spaceunits {
            ctx.spawn(self.create_ship_config(ctx.clone(), ship)).await;
        }
    }

    /// Visit a ship config, emitting a config file and
    #[instrument(err, skip_all, fields(ship.name = ship.name))]
    async fn create_ship_config(
        &'p self,
        ctx: ProcessorContext<'p>,
        ship: SpaceUnitXml,
    ) -> anyhow::Result<()> {
        const MISSING_MODEL: &str = "model_missing.alo";
        let model_output_path = match ship.space_model_name {
            None => {
                warn!("Ship {} has no model", ship.name);
                MISSING_MODEL.into()
            }
            Some(ref model) if model.is_empty() => {
                warn!("Model name for ship {} is empty", ship.name);
                MISSING_MODEL.into()
            }
            Some(ref model) => self.models.find_model(&ctx, model).await?,
        };
        let dest = self.ship_name_to_file(&*ship.name);

        let config = ShipConfig {
            name: ship.name.to_string(),
            model: model_output_path.into(),
            scale: ship.scale_factor.unwrap_or(1.0),
        };

        ctx.spawn(self.output.generate_conf(config, dest)).await;

        Ok(())
    }

    /// Converts a ship name to a config file path.
    fn ship_name_to_file(&self, name: impl Into<String>) -> PathBuf {
        let mut name = name.into();
        name.make_ascii_lowercase();

        let mut path = self.prefix.to_path_buf();
        path.push("config");
        path.push("ships");
        path.push(name);
        path.set_extension("ship.ron");
        path
    }
}
