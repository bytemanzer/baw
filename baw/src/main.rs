use std::path::PathBuf;

use bevy::prelude::*;
use bevy_alo::{AloBone, AloPlugin};
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
        .add_systems(Startup, setup)
        .add_systems(Update, draw_bones)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, args: Res<Args>) {
    // light
    commands.spawn((
        PointLight {
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
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
