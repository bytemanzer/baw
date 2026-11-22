use bevy::prelude::*;

pub use model::ModelAsset;
pub use scene::GameObjectScene;

mod loader;
mod model;
mod scene;

pub(crate) struct GameObjectScenePlugin;

impl Plugin for GameObjectScenePlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_loader(loader::GameObjectLoader);
    }
}
