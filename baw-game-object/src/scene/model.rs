use baw_scene_util::with_template;
use bevy::asset::{Asset, AssetPath};
use bevy::reflect::Reflect;
use bevy::scene::{
    ResolveContext, ResolveSceneError, ResolvedScene, SceneDependencies, SceneList, ScenePatch,
};
use bevy::transform::components::Transform;
use serde::{Deserialize, Serialize};

/// Serializable scene type representing essentially `EntityScene<(CachedSceneAsset, ScaleScene)>`
#[derive(Serialize, Deserialize, Asset, Reflect)]
pub struct ModelAsset {
    /// Model to spawn as a child of the current scene.
    pub model: AssetPath<'static>,
    /// Scale factor to apply to the whole model.
    #[serde(default = "scale::default")]
    #[serde(skip_serializing_if = "scale::is_default")]
    pub scale: f32,
}

/// Utilities for serializing configured scale.
mod scale {
    /// Gets the default value for the scale field.
    pub const fn default() -> f32 {
        1.0
    }

    /// Gets the default value of the
    pub const fn is_default(val: &f32) -> bool {
        *val == default()
    }
}

impl ModelAsset {
    /// Resolve the child scene, that is the scene passed here is the child scene that the model is
    /// being spawned within, not the parent.
    fn resolve_child(
        self,
        context: &mut ResolveContext,
        scene: &mut ResolvedScene,
    ) -> Result<(), ResolveSceneError> {
        // This part is copied from CachedSceneAsset.
        if let Some(handle) = context.assets.get_handle::<ScenePatch>(&self.model)
            && let Some(scene_patch) = context.patches.get(&handle)
        {
            scene.include_cached(handle)?;
            context.cached = Some(scene_patch);
        } else {
            return Err(ResolveSceneError::MissingSceneDependency(self.model));
        }
        // This part is copied from ScaleScene
        with_template!(context, scene, Transform, |trans| {
            trans.scale *= self.scale;
        });
        Ok(())
    }
}

impl SceneList for ModelAsset {
    fn resolve_list(
        self,
        context: &mut ResolveContext,
        scenes: &mut Vec<ResolvedScene>,
    ) -> Result<(), ResolveSceneError> {
        // This part is copied from EntityScene.
        let mut resolved_scene = ResolvedScene::default();
        self.resolve_child(context, &mut resolved_scene)?;
        scenes.push(resolved_scene);
        Ok(())
    }

    fn register_dependencies(&self, deps: &mut SceneDependencies) {
        deps.register::<ScenePatch>(self.model.clone());
    }
}
