//! Provides marker components for various game object behaviors in BaW.
//!
//! Also provides an asset format and asset loader for BAW Game Objects, but that's mostly a
//! temporary stand-in for Bevy scenes, and can be replaced with BSN once an on-disk BSN notation is
//! stabilized.

use bevy::prelude::*;

pub mod scene;

/// Configures the GameObject components.
pub struct GameObjectPlugin;

impl Plugin for GameObjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(scene::GameObjectScenePlugin);
    }
}
