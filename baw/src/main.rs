use std::path::PathBuf;

use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_alo::AloPlugin;
use bevy_alo::components::AloBone;
use bevy_panorbit_camera::{PanOrbitCamera, PanOrbitCameraPlugin};
use clap::Parser;

#[derive(Parser, Resource)]
struct Args {
    #[arg(long)]
    model: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AloPlugin)
        .add_plugins(PanOrbitCameraPlugin)
        .insert_resource(args)
        .insert_resource(ClearColor(Srgba::BLACK.into()))
        .add_systems(Startup, setup)
        .add_systems(Update, draw_bones)
        .add_systems(Update, pulse_colors)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, args: Res<Args>) {
    // light
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: light_consts::lux::AMBIENT_DAYLIGHT,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            maximum_distance: 2000.0,
            first_cascade_far_bound: 400.0,
            minimum_distance: 1.0,
            ..Default::default()
        }.build(),
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // camera
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-25.0, 45.0, 90.0).looking_at(Vec3::ZERO, Vec3::Y),
        PanOrbitCamera::default(),
    ));

    // selected model
    if let Some(ref model) = args.model {
        let path = if let Ok(path) = model.strip_prefix("baw/assets") {
            path.to_owned()
        } else if let Ok(path) = model.strip_prefix("assets") {
            path.to_owned()
        } else {
            model.clone()
        };
        commands.spawn(SceneRoot(asset_server.load(path)));
    }
}

fn draw_bones(mut gizmos: Gizmos, bones: Query<&GlobalTransform, With<AloBone>>) {
    for bone in bones {
        gizmos.sphere(bone.translation(), 0.1, Srgba::GREEN);
    }
}

fn pulse_colors() {}
