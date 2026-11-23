//! Provides helpers for working with Petroglyph chunked files.

extern crate self as petro_chunked;

pub use byteblock::{
    ConvertBytes, FromByteBlock, FromFixedByteBlock, TakeBytes, U8Bool, U32Bool, U32Usize,
    U32UsizeVec,
};
pub use chunks::{Chunk, ChunkContent, ChunkStreamItem, ChunkStructure, iter_chunks};
pub use error::ChunkError;
pub use minichunk::{MiniChunk, MiniChunkStructure, iter_mini_chunks};
#[cfg(feature = "derive")]
pub use petro_chunked_derive::{FromByteBlock, FromChunks, FromMiniChunks};

pub mod byteblock;
pub mod chunks;
pub mod content;
mod error;
pub mod minichunk;
