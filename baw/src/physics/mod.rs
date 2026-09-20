use avian3d::PhysicsPlugins;
#[cfg(feature = "physics-debug")]
use avian3d::debug_render::PhysicsDebugPlugin;
use bevy::app::{FixedPostUpdate, PluginGroup, PluginGroupBuilder};

/// Configure physics for BAW.
pub(crate) struct BawPhysicsPlugins {
    base: PhysicsPlugins,
}

impl BawPhysicsPlugins {
    /// Create a new [BawPhysicsPlugin] with default settings.
    pub fn new() -> Self {
        Self {
            base: PhysicsPlugins::new(FixedPostUpdate),
        }
    }
}

impl PluginGroup for BawPhysicsPlugins {
    fn build(self) -> PluginGroupBuilder {
        let builder = self.base.build();

        #[cfg(feature = "physics-debug")]
        let builder = builder.add(PhysicsDebugPlugin);

        builder
    }
}
