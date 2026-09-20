use std::path::PathBuf;

use baw::BawPlugin;
use bevy::prelude::*;
use clap::Parser;

#[derive(Parser, Resource)]
struct Args {
    #[arg(long)]
    model: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    App::new()
        .add_plugins(BawPlugin::new())
        .insert_resource(args)
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, args: Res<Args>) {
    // selected model
    if let Some(ref model) = args.model {
        let path = if let Ok(path) = model.strip_prefix("baw/assets") {
            path.to_owned()
        } else if let Ok(path) = model.strip_prefix("assets") {
            path.to_owned()
        } else {
            model.clone()
        };
        commands.spawn(WorldAssetRoot(asset_server.load(path)));
    }
}
