use std::io;

use thiserror::Error;

/// Error from loading an ALO asset.
#[derive(Error, Debug)]
pub enum AloError {
    #[error("Malformed ALO file: {0}")]
    AloFile(#[from] petro_alo::AloError),
    #[error("Error reading ALO file: {0}")]
    Io(#[from] io::Error),
    #[error("Vertex format {0:?} is not supported")]
    UnsupportedVertextFormat(String),
}
