use baw_config::units::ships::ShipConfig;
use baw_scene_util::{ScaleScene, with_template};
use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::prelude::*;
use bevy::scene::{
    CachedSceneAsset, EntityScene, ResolveContext, ResolveSceneError, ResolvedScene,
    SceneDependencies, ScenePatch,
};

#[derive(Default, TypePath)]
pub(crate) struct ShipLoader;

impl AssetLoader for ShipLoader {
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
        let config: ShipConfig = ron::de::from_bytes(&file)?;

        let ship = ShipScene {
            name: config.name.clone(),
            model: EntityScene((
                CachedSceneAsset::from(config.model),
                ScaleScene(config.scale),
            )),
        };

        Ok(ScenePatch::load_with(load_context, ship))
    }

    fn extensions(&self) -> &[&str] {
        &["ship.ron"]
    }
}

/// Asset representing a prepared ship.
struct ShipScene {
    /// Name of this ship type.
    name: String,
    /// Main model to load for this ship. This will be spawned as a child of the ship.
    model: EntityScene<(CachedSceneAsset, ScaleScene)>,
}

impl Scene for ShipScene {
    fn resolve(
        self,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        with_template!(context, scene, Name, |name| {
            name.set(self.name);
        });
        with_template!(context, scene, Transform);
        with_template!(context, scene, Visibility, |vis| {
            *vis = Visibility::Inherited;
        });

        let children = scene.get_or_insert_related_resolved_scenes::<ChildOf>();
        self.model.resolve_list(context, &mut children.scenes)?;
        Ok(())
    }

    fn register_dependencies(&self, deps: &mut SceneDependencies) {
        self.model.register_dependencies(deps);
    }
}
