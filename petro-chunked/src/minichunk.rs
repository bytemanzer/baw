use crate::ChunkError;
use crate::byteblock::{ConvertBytes, FromByteBlock, TakeBytes};
use crate::error::ChunkErrorContext;

/// Interprets the given bytes as containing mini-chunks and gets an iterator over them.
pub fn iter_mini_chunks<'b>(data: &'b [u8]) -> MiniChunksIter<'b> {
    MiniChunksIter { data }
}

/// Iterator over [`MiniChunk`]s in some source data.
pub struct MiniChunksIter<'b> {
    /// Source bytes to extract chunks from.
    data: &'b [u8],
}

impl<'b> MiniChunksIter<'b> {
    /// Peek the ID of the next mini-chunk, if there are any left.
    pub fn peek_id(&self) -> Result<Option<u8>, ChunkError> {
        let mut data = self.data;
        data.take()
    }

    /// Get the next chunk if it matches the given matcher otherwise error.
    pub fn expect_chunk<M: MiniChunkIdMatcher>(
        &mut self,
        matcher: M,
    ) -> Result<MiniChunk<'b>, ChunkError> {
        match self.next() {
            None => Err(ChunkError::EndOfChunks),
            Some(Err(e)) => Err(e),
            Some(Ok(chunk)) if matcher.matches(chunk.id()) => Ok(chunk),
            Some(Ok(chunk)) => Err(ChunkError::UnexpectedChunk {
                id: chunk.id() as u32,
            }),
        }
    }

    /// Get the next chunk if it matches the given matcher, otherwise return None.
    pub fn try_expect_chunk<M: MiniChunkIdMatcher>(
        &mut self,
        matcher: M,
    ) -> Result<Option<MiniChunk<'b>>, ChunkError> {
        if let Some(id) = self.peek_id()?
            && matcher.matches(id)
        {
            return self.expect_chunk(id).map(Some);
        }
        Ok(None)
    }

    /// Produce an error if there are chunks/data left over.
    pub fn expect_empty(mut self) -> Result<(), ChunkError> {
        match self.next() {
            Some(Err(e)) => Err(e),
            Some(Ok(chunk)) => Err(ChunkError::UnexpectedChunk {
                id: chunk.id() as u32,
            }),
            None => Ok(()),
        }
    }
}

impl<'b> Iterator for MiniChunksIter<'b> {
    type Item = Result<MiniChunk<'b>, ChunkError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            None
        } else {
            Some(self.data.take())
        }
    }
}

/// Utility to match mini-chunk IDs.
pub trait MiniChunkIdMatcher {
    /// Return true if the chunk ID matches this matcher.
    fn matches(&self, id: u8) -> bool;
}

impl MiniChunkIdMatcher for u8 {
    fn matches(&self, id: u8) -> bool {
        *self == id
    }
}

impl<const SIZE: usize> MiniChunkIdMatcher for [u8; SIZE] {
    fn matches(&self, id: u8) -> bool {
        self.contains(&id)
    }
}

impl MiniChunkIdMatcher for &[u8] {
    fn matches(&self, id: u8) -> bool {
        self.contains(&id)
    }
}

impl<F: Fn(u8) -> bool> MiniChunkIdMatcher for F {
    fn matches(&self, id: u8) -> bool {
        self(id)
    }
}

/// Represents a single mini-chunk from a Petroglyph chunked file.
pub struct MiniChunk<'b> {
    /// ID of this chunk.
    id: u8,
    /// Unparsed content of this chunk.
    content: &'b [u8],
}

impl<'b> MiniChunk<'b> {
    /// Gets the ID of the mini-chunk.
    pub fn id(&self) -> u8 {
        self.id
    }

    /// Gets the contents of the Mini Chunk.
    pub fn content(&self) -> &'b [u8] {
        self.content
    }
}

impl<'b> ConvertBytes<'b> for MiniChunk<'b> {
    fn convert<T: FromByteBlock<'b>>(&self) -> Result<T, ChunkError> {
        self.content.convert().err_to_mini_chunk_data(self.id)
    }
}

impl<'b> FromByteBlock<'b> for MiniChunk<'b> {
    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), ChunkError> {
        let (id, len): (u8, u8) = bytes.take().err_to_mini_header()?;
        let content = bytes.take_bytes(len as usize).err_to_mini_chunk_data(id)?;
        let chunk = MiniChunk { id, content };
        Ok((chunk, bytes))
    }
}
