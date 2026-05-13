use crate::ChunkError;
use crate::byteblock::{FromByteBlock, FromFixedByteBlock, TakeBytes};
use crate::content::DataOrMiniChunks;
use crate::error::ChunkErrorContext;

/// Interprets the bytes as containing chunks and gets an iterator over them.
pub fn iter_chunks<'b>(data: &'b [u8]) -> ChunksIter<'b> {
    ChunksIter { data }
}

/// Iterator over [`Chunk`]s in some source data.
#[derive(Debug)]
pub struct ChunksIter<'b> {
    /// Source bytes to extract chunks from.
    data: &'b [u8],
}

impl<'b> ChunksIter<'b> {
    /// Peek the ID of the next chunk, if there are any left.
    pub fn peek_id(&self) -> Result<Option<u32>, ChunkError> {
        let mut data = self.data;
        data.take()
    }

    /// Get the next chunk if it matches the given matcher, otherwise error.
    pub fn expect_chunk<M: ChunkIdMatcher>(&mut self, matcher: M) -> Result<Chunk<'b>, ChunkError> {
        match self.next() {
            None => Err(ChunkError::EndOfChunks),
            Some(Err(e)) => Err(e),
            Some(Ok(chunk)) if matcher.matches(chunk.id()) => Ok(chunk),
            Some(Ok(chunk)) => Err(ChunkError::UnexpectedChunk { id: chunk.id() }),
        }
    }

    /// Get the next chunk if it matches the given matcher, otherwise return None.
    pub fn try_expect_chunk<M: ChunkIdMatcher>(
        &mut self,
        matcher: M,
    ) -> Result<Option<Chunk<'b>>, ChunkError> {
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
            Some(Ok(chunk)) => Err(ChunkError::UnexpectedChunk { id: chunk.id() }),
            None => Ok(()),
        }
    }
}

impl<'b> Iterator for ChunksIter<'b> {
    type Item = Result<Chunk<'b>, ChunkError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            None
        } else {
            Some(self.data.take())
        }
    }
}

/// Utility to match chunk IDs.
pub trait ChunkIdMatcher {
    /// Return true if the chunk ID matches this matcher.
    fn matches(&self, id: u32) -> bool;
}

impl ChunkIdMatcher for u32 {
    fn matches(&self, id: u32) -> bool {
        *self == id
    }
}

impl<const SIZE: usize> ChunkIdMatcher for [u32; SIZE] {
    fn matches(&self, id: u32) -> bool {
        self.contains(&id)
    }
}

impl ChunkIdMatcher for &[u32] {
    fn matches(&self, id: u32) -> bool {
        self.contains(&id)
    }
}

impl<F: Fn(u32) -> bool> ChunkIdMatcher for F {
    fn matches(&self, id: u32) -> bool {
        self(id)
    }
}

/// Indicates whether a chunk contains data or more chunks.
#[derive(Copy, Clone, Debug)]
enum ChunkContentKind {
    /// Chunk contains data or mini-chunks.
    Data,
    /// Chunk contains more chunks.
    Chunks,
}

/// Content type of a Chunk.
#[derive(Debug)]
pub enum ChunkContent<'b> {
    /// The chunk contains data or mini-chunks.
    Data(DataOrMiniChunks<'b>),
    /// The chunk contains child chunks.
    Chunks(ChunksIter<'b>),
}

impl<'b> ChunkContent<'b> {
    /// If the chunk content is data, returns it, otherwise returns none.
    pub fn data(self) -> Result<DataOrMiniChunks<'b>, ChunkError> {
        match self {
            Self::Data(data) => Ok(data),
            _ => Err(ChunkError::ExpectedData),
        }
    }

    /// If the chunk content is more chunks, returns an iterator over the child chunks.
    pub fn children(self) -> Result<ChunksIter<'b>, ChunkError> {
        match self {
            Self::Chunks(chunks) => Ok(chunks),
            _ => Err(ChunkError::ExpectedChunks),
        }
    }
}

/// Represents a single chunk from a Petroglyph chunked file.
pub struct Chunk<'b> {
    /// ID of this chunk.
    id: u32,
    /// Which content type this chunk contained.
    kind: ChunkContentKind,
    /// Unparsed content of this chunk.
    content: &'b [u8],
}

impl<'b> Chunk<'b> {
    /// Gets the ID of this chunk.
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn content(&self) -> ChunkContent<'b> {
        match self.kind {
            ChunkContentKind::Data => ChunkContent::Data(DataOrMiniChunks::new(self.content)),
            ChunkContentKind::Chunks => ChunkContent::Chunks(iter_chunks(self.content)),
        }
    }

    /// If the chunk contains plain data or mini chunks, returns the data. If the chunk contains
    /// more chunks, return an error.
    pub fn data(&self) -> Result<DataOrMiniChunks<'b>, ChunkError> {
        self.content().data()
    }

    /// If the chunk contains child chunks but not mini chunks, returns an iterator over the
    /// children. Otherwise returns an error.
    pub fn children(&self) -> Result<ChunksIter<'b>, ChunkError> {
        self.content().children()
    }
}

/// Helper to unpack the chunk kind from the chunk length.
struct LenKind {
    len: usize,
    kind: ChunkContentKind,
}

impl<'b> FromFixedByteBlock<'b> for LenKind {
    const SIZE: usize = <u32 as FromFixedByteBlock<'b>>::SIZE;

    fn from_bytes(bytes: &'b [u8]) -> Self {
        let field = <u32 as FromFixedByteBlock<'b>>::from_bytes(bytes);
        // Fail if pointer size is less than 4 bytes and the value is too large for usize.
        let len = (field & 0x7fff_ffff).try_into().unwrap();
        let kind = if field & 0x8000_0000 != 0 {
            ChunkContentKind::Chunks
        } else {
            ChunkContentKind::Data
        };
        Self { len, kind }
    }
}

impl<'b> FromByteBlock<'b> for Chunk<'b> {
    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), ChunkError> {
        let (id, LenKind { len, kind }) = bytes.take().err_to_header()?;
        let content = bytes.take_bytes(len).err_to_data(id)?;
        let chunk = Chunk { id, kind, content };
        Ok((chunk, bytes))
    }
}
