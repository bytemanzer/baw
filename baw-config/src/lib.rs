use bevy::app::{PluginGroup, PluginGroupBuilder};

use crate::ship::ShipPlugin;

/// Helper to set an optional field from a base type's value, if present.
macro_rules! apply_base {
    ($self:ident, $base:ident, [$($field:ident),* $(,)?]) => {
        $(
        if $self.$field.is_none()  {
            $self.$field = $base.$field.clone();
        }
        )*
    };
}

pub mod ship;

/// Configures support for the various config asset types.
pub struct ConfigPlugins;

impl PluginGroup for ConfigPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<ConfigPlugins>().add(ShipPlugin)
    }
}
