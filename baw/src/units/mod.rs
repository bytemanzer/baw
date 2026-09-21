use bevy::app::{App, Plugin};
use bevy::asset::AssetApp;

use crate::units::ship::ShipLoader;

pub mod ship;

/// Configures support for the various config asset types.
pub(crate) struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_loader(ShipLoader);
    }
}
