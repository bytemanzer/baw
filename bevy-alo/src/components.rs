use bevy::prelude::*;

pub struct AloComponentsPlugin;

impl Plugin for AloComponentsPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<AloBone>()
            .register_type::<AloInitialVisibility>()
            .register_type::<AloFaceCamera>()
            .register_type::<AloBillboardOut>()
            .register_type::<AloCamera>()
            .register_type::<AloMesh>();
    }
}

/// Marker component for entities that are the bones of an Alo skeleton.
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component)]
pub struct AloBone;

/// Stores the initial visibility of an ALO item.
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component)]
pub struct AloInitialVisibility {
    /// If true, the ALO item is initially hidden.
    pub hidden: bool,
}

/// Billboard facing the camera. Optionally offset from the parent position and optionally keep a
/// vertical.
#[derive(Component, Reflect, Clone, Default)]
#[require(AloBillboardOut)]
#[reflect(Component)]
pub struct AloFaceCamera {
    /// If true, only rotate around the y axis, keeping the object vertical.
    pub keep_vertical: bool,
    /// Offset relative to the parent.
    pub local_offset: Vec3,
    /// If true, rotate around the parent instead of the local offset.
    pub rotate_around_parent: bool,
}

/// Output component for AloFaceCamera. Gets copied to transform after running billboarding.
#[derive(Component, Reflect, Clone, Default)]
#[reflect(Component)]
pub(crate) struct AloBillboardOut {
    /// Billboarded transform.
    pub(crate) new_transform: Transform,
}

/// Marker component for entities that are the "meshes" of an Alo file.
#[derive(Default, Clone, Component, Reflect)]
#[reflect(Component)]
pub struct AloMesh;

/// Marker component for the entity which AloFaceCamera billboards point towards.
#[derive(Component, Reflect)]
#[reflect(Component)]
pub struct AloCamera;
