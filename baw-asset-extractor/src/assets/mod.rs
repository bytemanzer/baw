//! Provides different asset sources.

pub use asset::{Asset, AssetReader};
pub use file_system_view::{Assets, FileSystemView};

mod asset;
mod file;
mod file_system_view;
mod meg;
