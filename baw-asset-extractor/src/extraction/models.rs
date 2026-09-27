use std::path::PathBuf;

use anyhow::bail;
use petro_meg::path::{MegPath, meg_path};
use tracing::{Level, instrument};

use crate::assets::FileSystemView;
use crate::processing::{FileWriter, ProcessorContext};

pub struct ModelExtractor<'a> {
    fs: &'a FileSystemView,
    output: &'a FileWriter<'a>,
    prefix: PathBuf,
}

impl<'a> ModelExtractor<'a> {
    /// Create a new config extractor that reads from the specified filesystem and writes to the
    /// given file output.
    pub fn new(fs: &'a FileSystemView, output: &'a FileWriter<'a>) -> Self {
        Self {
            fs,
            output,
            prefix: PathBuf::new(),
        }
    }

    /// Sets the path prefix to use for output files.
    pub fn with_prefix(mut self, prefix: impl Into<PathBuf>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Find a model file, dispatching a copy of the model to the output. Returns the path that the
    /// model will have in the output directory, including any relevant prefix.
    #[instrument(skip_all, level = Level::DEBUG)]
    pub async fn find_model(
        &'a self,
        ctx: &ProcessorContext<'a>,
        name: &MegPath,
    ) -> anyhow::Result<PathBuf> {
        const MODELS: &MegPath = meg_path!("Data\\Art\\Models");
        let path = MODELS.join(name);
        let asset = match self.fs.get(&path) {
            Some(asset) => asset,
            None => bail!("Model {name} not found in {MODELS}"),
        };

        let file_name = match name.file_name() {
            None => bail!("model file name is empty"),
            Some(name) => name.to_lowercase().into_string(),
        };
        let mut output_model_file = self.prefix.join("models");
        output_model_file.push(file_name);

        ctx.spawn(self.output.copy_asset(asset, output_model_file.clone()))
            .await;

        // TODO: read the model and find needed textures; emit a meta file mapping those textures.
        Ok(output_model_file)
    }
}
