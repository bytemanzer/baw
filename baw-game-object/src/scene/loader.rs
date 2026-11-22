use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::prelude::*;
use bevy::scene::ScenePatch;

use crate::scene::GameObjectScene;

#[derive(Default, TypePath)]
pub(crate) struct GameObjectLoader;

impl AssetLoader for GameObjectLoader {
    type Asset = ScenePatch;
    type Error = anyhow::Error;
    type Settings = ();

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut file = Vec::new();
        reader.read_to_end(&mut file).await?;
        let scene: GameObjectScene = ron::de::from_bytes(&file)?;
        Ok(ScenePatch::load_with(load_context, scene))
    }

    fn extensions(&self) -> &[&str] {
        // Game Object Scene
        &["gos"]
    }
}
