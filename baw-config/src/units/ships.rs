use bevy_asset::{Asset, AssetPath};
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

use crate::serdeutil::scale;

/// Defines configuration for a ship.
#[derive(Serialize, Deserialize, Asset, Reflect)]
pub struct ShipConfig {
    /// Name that identifies this ship type.
    pub name: String,
    /// Model that represents this ship.
    pub model: AssetPath<'static>,
    /// Scale factor to apply to the whole model.
    #[serde(default = "scale::default")]
    #[serde(skip_serializing_if = "scale::is_default")]
    pub scale: f32,
}
