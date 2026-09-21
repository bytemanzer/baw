use std::io;

use petro_alo::alternatives::{Lod, LodGroup};
use thiserror::Error;

/// Error from loading an ALO asset.
#[derive(Error, Debug)]
pub enum AloError {
    #[error("Malformed ALO file: {0}")]
    AloFile(#[from] petro_alo::AloError),
    #[error("Error reading ALO file: {0}")]
    Io(#[from] io::Error),
    #[error("Shader param {param} had an unexpected type {value_type:?}, expected {expected}")]
    UnexpectedShaderValueType {
        param: &'static str,
        value_type: &'static str,
        expected: &'static str,
    },
    #[error("Mesh duplicate LOD {lod:?} for group {group:?}")]
    DuplicateLod { group: LodGroup, lod: Lod },
    #[error("Mesh had an unkown LOD {lod:?} for group {group:?}")]
    UnknownLod { group: LodGroup, lod: Lod },
    /// Bones must specify a parent with an index that is strictly less than their own index in the
    /// skeleton.
    #[error("{category} at index {item_idx} specified invalid parent index {parent_idx}")]
    InvalidParentIndex {
        category: &'static str,
        item_idx: usize,
        parent_idx: usize,
    },
    /// All bones besides the root are required to specify a parent.
    #[error("Bone at index {bone_idx} is missing a parent")]
    MissingParent { bone_idx: usize },
}
