use std::io;

use thiserror::Error;

/// Error from loading an ALO asset.
#[derive(Error, Debug)]
pub enum AloError {
    #[error("Malformed ALO file: {0}")]
    AloFile(#[from] petro_alo::AloError),
    #[error("Error reading ALO file: {0}")]
    Io(#[from] io::Error),
    #[error("Unable to create a collider from a collision mesh")]
    ConvexDecompositionFailed,
}
