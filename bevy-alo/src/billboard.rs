//! Provides systems implementing ALO billboarding.
use std::f32::consts::PI;

use bevy::prelude::*;

use crate::components::{AloBillboardOut, AloCamera, AloFaceCamera};

/// Plugin for Alo billboard support.
pub struct AloBillboardPlugin;

impl Plugin for AloBillboardPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            billboard_face_camera
                .in_set(AloBillboardSystemSet)
                .before(apply_billboard_transforms),
        );
        app.add_systems(
            PostUpdate,
            apply_billboard_transforms
                .in_set(AloBillboardSystemSet)
                .before(TransformSystems::Propagate),
        );
    }
}
/// System set to allow ordering of ALO billboards.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct AloBillboardSystemSet;

/// Computes the billboarded transform for entities which face the camera.
fn billboard_face_camera(
    global: TransformHelper,
    camera: Single<Entity, With<AloCamera>>,
    billboards: Query<(&mut AloBillboardOut, &AloFaceCamera, Option<&ChildOf>)>,
) {
    if billboards.is_empty() {
        return;
    }
    let camera_pos = match global.compute_global_transform(*camera) {
        Ok(trans) => trans.translation_vec3a(),
        Err(e) => {
            warn!(
                "Not applying camera billboarding; could not compute billboard camera transform: {e}"
            );
            return;
        }
    };
    for (mut out, billboard, parent) in billboards {
        let parent_transform = match parent.map(|ChildOf(p)| global.compute_global_transform(*p)) {
            None => GlobalTransform::IDENTITY,
            Some(Ok(t)) => t,
            Some(Err(e)) => {
                warn!(
                    "Skipping entity for camera billboarding; coult not compute its parent position: {e}"
                );
                continue;
            }
        };
        let world_to_parent = parent_transform.affine().inverse();
        let global_up_in_parent_space = Dir3A::new_unchecked(
            world_to_parent
                .transform_vector3a(Vec3A::Y)
                .normalize_or(Vec3A::Y),
        );
        let camera_pos_in_parent_space = world_to_parent.transform_point3a(camera_pos);
        let (target_pos, up) = if billboard.keep_vertical {
            let projection_on_up =
                camera_pos_in_parent_space.project_onto(global_up_in_parent_space.as_vec3a());
            let target_pos = camera_pos_in_parent_space - projection_on_up;
            (target_pos, global_up_in_parent_space)
        } else {
            (camera_pos_in_parent_space, Dir3A::Y)
        };
        out.new_transform = if billboard.rotate_around_parent {
            let mut new_transform = Transform::IDENTITY.looking_at(target_pos.into(), up);
            // Flip the direction because looking at points us the wrong way.
            new_transform.rotate_local_y(PI);
            // Now use the transform to transform the offset based on the billboarding rotation.
            new_transform.translation = new_transform.transform_point(billboard.local_offset);
            new_transform
        } else {
            let mut new_transform = Transform::from_translation(billboard.local_offset)
                .looking_at(target_pos.into(), up);
            // Flip the direction because looking at points us the wrong way.
            new_transform.rotate_local_y(PI);
            new_transform
        };
    }
}

/// Propagate computed billboard transforms.
fn apply_billboard_transforms(billboards: Query<(&mut Transform, &AloBillboardOut)>) {
    for (mut trans, billboard_out) in billboards {
        *trans = billboard_out.new_transform;
    }
}
