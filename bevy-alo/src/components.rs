use bevy::prelude::*;
use petro_alo::skeleton::BillboardMode;

pub struct AloComponentsPlugin;

impl Plugin for AloComponentsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AloBone>()
            .register_type::<AloBillboard>()
            .register_type::<AloMesh>();
    }
}

/// Marker component for entities that are the bones of an Alo skeleton.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct AloBone;

/// Holds the [`BillboardMode`] of an ALO bone.
#[derive(Component, Reflect, Clone)]
#[reflect(Component, opaque)]
pub struct AloBillboard(pub BillboardMode);

/// Marker component for entities that are the "meshes" of an Alo file.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct AloMesh;
