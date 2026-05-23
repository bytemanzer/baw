use bevy::prelude::*;

pub use error::AloError;

use crate::components::AloComponentsPlugin;
use crate::materials::AloMaterialsPlugin;
use crate::models::AloModelsPlugin;

pub mod components;
mod error;
pub mod labels;
pub mod materials;
pub mod models;

/// Plugin which adds the ALO asset loader.
pub struct AloPlugin;

impl Plugin for AloPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((AloMaterialsPlugin, AloComponentsPlugin))
            .add_plugins(AloModelsPlugin);
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
