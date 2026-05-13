use petro_chunked::ChunkError;
use thiserror::Error;

#[derive(Error, Debug)]
#[error(transparent)]
pub struct AloError {
    #[from]
    source: anyhow::Error,
}

/// Format errors specific to Alo files.
#[derive(Error, Debug)]
pub enum FormatError {
    #[error("Chunk {id:0>8x} unexpectedly found more than once")]
    DuplicateChunk { id: u32 },
    #[error("Out-of-order bone: bone {idx} has parent {parent}")]
    MisorderedBone { idx: usize, parent: usize },
    #[error("Expected at least one bone mapping but found 0")]
    MissingBoneMappings,
    #[error("Expected vertices but found none")]
    MissingVertices,
    #[error("Expected primitives but found none")]
    MissingPrimitives,
    #[error("Primitive index {idx} is out of range ({len} vertices available)")]
    PrimitiveIndexOutOfRange { idx: u16, len: usize },
    #[error("Expected either 0 or 1 for a boolean value, got {value}")]
    UnexpectedBool { value: u32 },
    #[error("Billboard mode {mode} was not recognized")]
    UnrecognizedBillboardMode { mode: u32 },
    #[error("Expected to find {expected} values, but found {actual}")]
    WrongContentCount { expected: usize, actual: usize },
    #[error("Bone index {idx} is not mapped to a bone.")]
    BoneNotMapped { idx: u32 },
    #[error("Bone mapping to index {idx} is out of range, only {count} bones.")]
    MappingIndexOutOfRange { idx: usize, count: usize },
    #[error("Connection mapping to bone {idx} out of range, only {count} bones.")]
    ConnBoneOutOfRange { idx: usize, count: usize },
    #[error("Connection mapping to object {idx} out of range, only {count} objects.")]
    ConnObjectOutOfRange { idx: usize, count: usize },
    #[error("Mesh {mesh_idx} connected to multiple bones")]
    MultiParentMesh { mesh_idx: usize },
    #[error("Vertex format {format:?} not recognized")]
    UnknownVertexFormat { format: String },
}

impl From<FormatError> for ChunkError {
    fn from(value: FormatError) -> Self {
        ChunkError::FormatError(anyhow::Error::new(value))
    }
}
