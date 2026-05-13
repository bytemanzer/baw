use std::io;

use bevy::asset::LoadDirectError;
use petro_alo::mesh::ShaderParam;
use thiserror::Error;

/// Error from loading an ALO asset.
#[derive(Error, Debug)]
pub enum AloError {
    #[error("Malformed ALO file: {0}")]
    AloFile(#[from] petro_alo::AloError),
    #[error("Error reading ALO file: {0}")]
    Io(#[from] io::Error),
    #[error("Unable to directly load a required sub-asset that we needed to manipulate")]
    RequiredSubAsset(#[from] LoadDirectError),
    #[error("Unable to create a collider from a collision mesh")]
    ConvexDecompositionFailed,
    #[error("Shader param {param} had an unexpected type {value:?}, expected {expected}")]
    UnexpectedShaderValueType {
        param: &'static str,
        value: ShaderParam,
        expected: &'static str,
    },
}
