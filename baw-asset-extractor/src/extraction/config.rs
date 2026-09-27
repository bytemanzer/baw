use std::path::PathBuf;

use anyhow::{Context as _, bail};
use petro_meg::path::{MegPath, meg_path};
use tracing::{Level, debug, instrument};

use crate::assets::{Asset, FileSystemView};
use crate::extraction::models::ModelExtractor;
use crate::extraction::xmlmodel::{FileListXml, GameObjectListXml, SpaceUnitXml};
use crate::processing::{FileWriter, ProcessorContext};
use crate::xml;

/// Extracts the game's configuration.
pub struct ConfigExtractor<'a> {
    /// Filesystem view to look up config files in.
    fs: &'a FileSystemView,
    /// Model extractor to use to pull models.
    models: &'a ModelExtractor<'a>,
    /// Output to write generated configs to.
    output: &'a FileWriter<'a>,
    /// Path prefix to prepend to any file names emitted.
    prefix: PathBuf,
}

impl<'a> ConfigExtractor<'a> {
    /// Create a new config extractor that reads from the specified filesystem and writes to the
    /// given file output.
    pub fn new(
        fs: &'a FileSystemView,
        models: &'a ModelExtractor<'a>,
        output: &'a FileWriter<'a>,
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
    #[instrument(skip_all, level = Level::DEBUG)]
    pub async fn extract_data_files(&'a self, ctx: ProcessorContext<'a>) -> anyhow::Result<()> {
        const GAME_OBJECT_FILES: &MegPath = meg_path!("Data\\XML\\GameObjectFiles.XML");
        let objectfiles = match self.fs.get(GAME_OBJECT_FILES) {
            Some(asset) => asset,
            None => bail!("{GAME_OBJECT_FILES} not found"),
        };
        let objectfiles = objectfiles
            .open_reader()
            .with_context(|| format!("failed to open {objectfiles:?}"))?;
        let objectfiles: FileListXml = xml::deserialize_xml_utf8_or_cp1252(objectfiles)?;

        const XML_DIR: &MegPath = meg_path!("Data\\XML");
        for file in objectfiles.files.iter() {
            let fullpath = XML_DIR.join(file);
            match self.fs.get(&fullpath) {
                Some(asset) => ctx.spawn(self.visit_data_file(ctx.clone(), asset)).await,
                None => debug!("Data file {file} not found in file overlays, skipping"),
            }
        }
        Ok(())
    }

    /// Visit a Data XML file, extracting any relevant items.
    #[instrument(skip_all, level = Level::DEBUG, fields(asset = asset.path().as_str()))]
    async fn visit_data_file(
        &'a self,
        ctx: ProcessorContext<'a>,
        asset: &'a Asset,
    ) -> anyhow::Result<()> {
        debug!("Visiting {}", asset.path());
        let gameobjects = asset
            .open_reader()
            .with_context(|| format!("failed to open {asset:?}"))?;
        let gameobjects: GameObjectListXml = xml::deserialize_xml_utf8_or_cp1252(gameobjects)
            .with_context(|| format!("failed to parse {asset:?}"))?;
        for ship in gameobjects.spaceunits {
            ctx.spawn(self.create_ship_config(ctx.clone(), ship)).await;
        }
        Ok(())
    }

    #[instrument(skip_all, level = Level::DEBUG, fields(ship.name = &ship.name))]
    async fn create_ship_config(
        &'a self,
        ctx: ProcessorContext<'a>,
        ship: SpaceUnitXml,
    ) -> anyhow::Result<()> {
        let model_output_path = match ship.space_model_name {
            Some(ref model) => Some(self.models.find_model(&ctx, model).await?),
            None => None,
        };
        debug!("real model path will be {:?}", model_output_path);
        Ok(())
    }
}
