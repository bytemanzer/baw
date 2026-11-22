use baw_scene_util::with_template;
use bevy::asset::Asset;
use bevy::camera::visibility::Visibility;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::name::Name;
use bevy::reflect::Reflect;
use bevy::scene::{
    ResolveContext, ResolveSceneError, ResolvedScene, Scene, SceneDependencies, SceneList,
};
use bevy::transform::components::Transform;
use serde::{Deserialize, Serialize};

use crate::scene::ModelAsset;

/// Asset representing a prepared GameObject scene.
#[derive(Serialize, Deserialize, Asset, Reflect)]
pub struct GameObjectScene {
    /// Name of this ship type.
    pub name: String,
    /// Model to use for this GameObject.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelAsset>,
}

impl Scene for GameObjectScene {
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
