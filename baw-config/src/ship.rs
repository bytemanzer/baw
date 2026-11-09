use anyhow::{Context as _, bail};
use baw_scene_util::with_template;
use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, AssetPath, LoadContext, LoadedAsset};
use bevy::prelude::*;
use bevy::scene::{
    CachedSceneAsset, EntityScene, ResolveContext, ResolveSceneError, ResolvedScene,
    SceneDependencies, ScenePatch,
};
use serde::{Deserialize, Serialize};

pub struct ShipPlugin;

impl Plugin for ShipPlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_loader(ShipLoader)
            .init_asset::<ShipConfig>();
    }
}

#[derive(Default, TypePath)]
pub(crate) struct ShipLoader;

/// Label for the "Config" sub-asset.
const CONFIG_LABEL: &str = "Config";

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
        let mut config: ShipConfig = ron::de::from_bytes(&file)?;

        if let Some(ref variant_of) = config.variant_of {
            let base_ship: LoadedAsset<ShipConfig> = load_context
                .load_builder()
                .load_value(variant_of.clone().with_label(CONFIG_LABEL))
                .await
                .with_context(|| {
                    format!(
                        "Failed to load base ship type {} for {}",
                        variant_of, config.name
                    )
                })?;

            config.apply_base(base_ship.get());
        }

        let model = match config.model {
            Some(ref model) => model,
            None => {
                bail!("Ship {} has no model", config.name);
            }
        };

        let ship = ShipScene {
            name: config.name.clone(),
            model: EntityScene((
                CachedSceneAsset::from(model),
                ScaleScene(config.scale.unwrap_or(1.0)),
            )),
        };

        load_context.add_labeled_asset(CONFIG_LABEL, config);

        Ok(ScenePatch::load_with(load_context, ship))
    }

    fn extensions(&self) -> &[&str] {
        &["ship.ron"]
    }
}

/// Defines configuration for a ship.
#[derive(Serialize, Deserialize, Asset, Reflect)]
pub struct ShipConfig {
    /// Name that identifies this ship type.
    pub name: String,
    /// If this ship configures a variant of an existing ship type, it is referenced here.
    pub variant_of: Option<AssetPath<'static>>,
    /// Model that represents this ship.
    pub model: Option<AssetPath<'static>>,
    /// Scale factor to apply to the whole model.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f32>,
}

impl ShipConfig {
    fn apply_base(&mut self, base: &ShipConfig) {
        apply_base!(self, base, [model, scale]);
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

/// Scales the scene by the given factor.
struct ScaleScene(f32);

impl Scene for ScaleScene {
    fn resolve(
        self,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        with_template!(context, scene, Transform, |trans| {
            trans.scale *= self.0;
        });
        Ok(())
    }
}
