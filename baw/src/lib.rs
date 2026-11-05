use std::path::Path;

use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_alo::AloPlugins;
use bevy_alo::billboard::AloBillboardSystemSet;
use bevy_alo::components::{AloBone, AloCamera};
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin, PanOrbitCameraSystemSet};

use crate::physics::BawPhysicsPlugins;
use crate::units::UnitsPlugin;

mod physics;
pub mod units;

/// Configures BAW.
pub struct BawPlugin {
    /// Ensures this is not constructible as BawPlugin {}.
    _private: (),
}

impl BawPlugin {
    /// Create a new empty/default BAW plugin.
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl Plugin for BawPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            DefaultPlugins,
            AloPlugins::new(),
            PanOrbitCameraPlugin,
            BawPhysicsPlugins::new(),
            UnitsPlugin,
        ))
        .insert_resource(ClearColor(Srgba::BLACK.into()))
        .configure_sets(
            PostUpdate,
            PanOrbitCameraSystemSet.before(AloBillboardSystemSet),
        )
        .add_systems(Startup, setup)
        .add_systems(Update, draw_bones);
    }
}

fn setup(mut commands: Commands) {
    // light
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            contact_shadows_enabled: true,
            illuminance: light_consts::lux::AMBIENT_DAYLIGHT,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            maximum_distance: 2000.0,
            first_cascade_far_bound: 400.0,
            minimum_distance: 1.0,
            ..Default::default()
        }
        .build(),
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-25.0, 45.0, 90.0).looking_at(Vec3::ZERO, Vec3::Y),
        AloCamera,
        PanOrbitCamera::default(),
    ));
}

fn draw_bones(
    mut gizmos: Gizmos,
    bones: Query<(&GlobalTransform, Option<&Name>), With<AloBone>>,
    camera: Single<&GlobalTransform, With<AloCamera>>,
) {
    for (transform, name) in bones {
        gizmos.sphere(transform.translation(), 0.1, Srgba::WHITE);
        gizmos.arrow(
            transform.translation(),
            transform.translation() + transform.forward().as_vec3(),
            Srgba::BLUE,
        );
        gizmos.arrow(
            transform.translation(),
            transform.translation() + transform.up().as_vec3(),
            Srgba::RED,
        );
        gizmos.arrow(
            transform.translation(),
            transform.translation() + transform.right().as_vec3(),
            Srgba::GREEN,
        );
        if let Some(name) = name {
            let offset: Vec3A = camera.up().to_vec3a() + camera.right().to_vec3a();
            gizmos.text(
                Isometry3d {
                    rotation: camera.rotation(),
                    translation: transform.translation_vec3a() + offset,
                },
                name.as_str(),
                1.0,
                Vec2 { x: -0.5, y: -0.5 },
                Srgba::WHITE,
            );
        }
    }
}
