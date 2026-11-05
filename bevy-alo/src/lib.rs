use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

pub use error::AloError;

use crate::billboard::AloBillboardPlugin;
use crate::components::AloComponentsPlugin;
use crate::materials::AloMaterialsPlugin;
use crate::models::AloModelsPlugin;

pub mod animations;
pub mod billboard;
pub mod components;
mod error;
pub mod labels;
pub mod materials;
pub mod models;

/// Plugin which adds the ALO asset loader.
#[derive(Default, Clone)]
pub struct AloPlugins {
    models_plugin: AloModelsPlugin,
}

impl AloPlugins {
    /// Create a new AloPlugin with no path prefixes set.
    pub const fn new() -> Self {
        Self {
            models_plugin: AloModelsPlugin::new(),
        }
    }
}

impl PluginGroup for AloPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(AloMaterialsPlugin)
            .add(AloComponentsPlugin)
            .add(self.models_plugin)
            .add(AloBillboardPlugin)
    }
}

const ALO_TO_BEVY: Mat4 =
    Mat4::from_cols(Vec4::X, Vec4::new(0.0, 0.0, -1.0, 0.0), Vec4::Y, Vec4::W);

/// Transform a slice of ALO vectors to Bevy space. Since this transform does no translation, this
/// works on both position vectors and direction vectors.
fn alo_to_bevy(mut vals: Vec<[f32; 3]>) -> Vec<[f32; 3]> {
    for point in &mut vals {
        *point = ALO_TO_BEVY
            .transform_vector3(Vec3::from_array(*point))
            .to_array();
    }
    vals
}
