//! Provides ship components and systems for BAW.

use bevy::prelude::*;

mod loader;

/// Configures support for the various config asset types.
pub struct ShipsPlugin;

impl Plugin for ShipsPlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_loader(loader::ShipLoader);
    }
}
