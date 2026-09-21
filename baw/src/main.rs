use std::path::PathBuf;

use baw::BawPlugin;
use bevy::prelude::*;
use bevy::scene::CachedSceneAsset;
use clap::Parser;

#[derive(Parser, Resource)]
struct Args {
    #[arg(long)]
    model: Option<PathBuf>,
    #[arg(long)]
    #[cfg(feature = "physics-debug")]
    physics_debug: bool,
}

fn main() {
    let args = Args::parse();
    let mut app = App::new();
    app.add_plugins(BawPlugin::new())
        .insert_resource(args)
        .add_systems(Startup, setup);

    #[cfg(feature = "physics-debug")]
    {
        app.add_systems(Startup, show_physics_gizmos);
    }

    app.run();
}

fn setup(mut commands: Commands, args: Res<Args>) {
    // selected model
    if let Some(ref model) = args.model {
        let path = if let Ok(path) = model.strip_prefix("baw/assets") {
            path.to_owned()
        } else if let Ok(path) = model.strip_prefix("assets") {
            path.to_owned()
        } else {
            model.clone()
        };
        commands.queue_spawn_scene(CachedSceneAsset(path.into()));
    }
}

#[cfg(feature = "physics-debug")]
fn show_physics_gizmos(mut config_store: ResMut<GizmoConfigStore>, args: Res<Args>) {
    use avian3d::debug_render::PhysicsGizmos;

    if args.physics_debug {
        config_store.config_mut::<PhysicsGizmos>().1.collider_color =
            Some(Srgba::rgb(1.0, 1.0, 0.0).into());
    }
}
