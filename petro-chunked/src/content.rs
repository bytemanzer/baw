//! Utilites for reading chunk contents.
use std::ops::Deref;

use crate::ChunkError;
use crate::byteblock::{ConvertBytes, FromByteBlock};
use crate::minichunk::{MiniChunksIter, iter_mini_chunks};

/// Content of a chunk which contains plain data or mini-chunks.
#[derive(Copy, Clone, Debug)]
pub struct DataOrMiniChunks<'b> {
    /// The raw data contents of the chunk.
    data: &'b [u8],
}

impl<'b> DataOrMiniChunks<'b> {
    pub(crate) fn new(data: &'b [u8]) -> Self {
        Self { data }
    }

    /// Treat this chunk's contents as plain data.
    pub fn as_data(&self) -> &'b [u8] {
        self.data
    }

    /// Treat this chunk's contents as mini-chunks.
    pub fn as_mini_chunks(&self) -> MiniChunksIter<'b> {
        iter_mini_chunks(self.data)
    }
}

impl<'b> Deref for DataOrMiniChunks<'b> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<'b> AsRef<[u8]> for DataOrMiniChunks<'b> {
    fn as_ref(&self) -> &[u8] {
        self.data
    }
}

impl<'b> ConvertBytes<'b> for DataOrMiniChunks<'b> {
    fn convert<T: FromByteBlock<'b>>(&self) -> Result<T, ChunkError> {
        self.data.convert()
    }
}
