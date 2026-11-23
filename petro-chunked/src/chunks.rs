use thiserror::Error;

use crate::ChunkError;
use crate::byteblock::{ConvertBytes, FromByteBlock, FromFixedByteBlock, TakeBytes};
use crate::content::DataOrMiniChunks;
use crate::error::ChunkErrorContext;
use crate::minichunk::MiniChunkStructure;

/// Interprets the bytes as containing chunks and gets an iterator over them.
pub fn iter_chunks<'b>(data: &'b [u8]) -> ChunksIter<'b> {
    ChunksIter { data }
}

/// Iterator over [`Chunk`]s in some source data.
#[derive(Debug, Clone)]
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

    /// Applies a structure collector to this chunk iterator.
    pub fn expect_struct<C: ChunkStructure<'b>>(self) -> Result<C, ChunkError> {
        C::collect(self)
    }

    /// Pulls a single stream item (such as a chunk-tagged enum or a sequence struct spanning
    /// one or more chunks) from this chunk iterator.
    pub fn expect_item<I: ChunkStreamItem<'b>>(&mut self) -> Result<I, ChunkError> {
        I::take_from_chunks(self)
    }

    /// Pulls a single stream item if the next chunk ID matches the item's starting chunk ID.
    pub fn try_expect_item<I: ChunkStreamItem<'b>>(&mut self) -> Result<Option<I>, ChunkError> {
        if let Some(id) = self.peek_id()?
            && I::matches_start(id)
        {
            return I::take_from_chunks(self).map(Some);
        }
        Ok(None)
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

    /// Reads this chunk's plain data content as `T`, wrapping any error with this chunk's ID.
    pub fn read_data<T: FromByteBlock<'b>>(&self) -> Result<T, ChunkError> {
        self.data()
            .and_then(|d| d.convert())
            .err_to_data(self.id)
    }

    /// Reads this chunk's content as a mini-chunk structure `M`, wrapping any error with this
    /// chunk's ID.
    pub fn read_mini_chunks<M: MiniChunkStructure<'b>>(&self) -> Result<M, ChunkError> {
        self.data()
            .and_then(|d| d.as_mini_chunks().expect_struct())
            .err_to_data(self.id)
    }

    /// Reads this chunk's child chunks as a chunk structure `C`, wrapping any error with this
    /// chunk's ID.
    pub fn read_children<C: ChunkStructure<'b>>(&self) -> Result<C, ChunkError> {
        self.children()
            .and_then(|c| c.expect_struct())
            .err_to_data(self.id)
    }
}

/// A structure which is produced by consuming an entire [`ChunksIter`].
pub trait ChunkStructure<'b>: Sized {
    fn collect(chunks: ChunksIter<'b>) -> Result<Self, ChunkError>;
}

/// An item which is produced by pulling one or more chunks from a [`ChunksIter`].
///
/// This is implemented by chunk-tagged enums and sequence-mode chunk structs, allowing them to be
/// embedded inside other chunk structures (including multi-chunk pairs like submeshes or particle
/// tracks).
pub trait ChunkStreamItem<'b>: Sized {
    /// Returns true if a chunk with `id` can start this item.
    fn matches_start(id: u32) -> bool;

    /// Pulls one instance of `Self` from the chunk iterator.
    fn take_from_chunks(chunks: &mut ChunksIter<'b>) -> Result<Self, ChunkError>;
}

/// Format errors when handling chunks as a map.
#[derive(Error, Debug)]
pub enum ChunkMapFormatError {
    #[error("Found unexpected Chunk {id:0>8x}")]
    UnexpectedChunk { id: u32 },
    #[error("Chunk {id:0>8x} ({name}) appeared more than once")]
    DuplicateChunk { id: u32, name: &'static str },
    #[error("Chunk {id:0>8x} ({name}) was required but not found")]
    MissingRequiredChunk { id: u32, name: &'static str },
}

/// Collection helper for required map-mode chunks.
#[doc(hidden)]
pub struct RequiredChunkEntry<T>(Option<T>);

impl<T> RequiredChunkEntry<T> {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn add_value(&mut self, id: u32, name: &'static str, value: T) -> Result<(), ChunkError> {
        if self.0.is_none() {
            self.0 = Some(value);
            Ok(())
        } else {
            Err(ChunkError::FormatError(
                ChunkMapFormatError::DuplicateChunk { id, name }.into(),
            ))
        }
    }

    pub fn finish(self, id: u32, name: &'static str) -> Result<T, ChunkError> {
        match self.0 {
            Some(val) => Ok(val),
            None => Err(ChunkError::FormatError(
                ChunkMapFormatError::MissingRequiredChunk { id, name }.into(),
            )),
        }
    }
}

/// Collection helper for optional or default map-mode chunks.
#[doc(hidden)]
pub struct OptionalChunkEntry<T>(Option<T>);

impl<T> OptionalChunkEntry<T> {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn add_value(&mut self, id: u32, name: &'static str, value: T) -> Result<(), ChunkError> {
        if self.0.is_none() {
            self.0 = Some(value);
            Ok(())
        } else {
            Err(ChunkError::FormatError(
                ChunkMapFormatError::DuplicateChunk { id, name }.into(),
            ))
        }
    }

    pub fn finish(self, _id: u32, _name: &str) -> Result<Option<T>, ChunkError> {
        Ok(self.0)
    }

    pub fn finish_or_default(self) -> Result<T, ChunkError>
    where
        T: Default,
    {
        Ok(self.0.unwrap_or_default())
    }

    pub fn finish_or_else(self, f: impl FnOnce() -> T) -> Result<T, ChunkError> {
        Ok(self.0.unwrap_or_else(f))
    }
}

/// Collection helper for repeated map-mode chunks.
#[doc(hidden)]
pub struct RepeatedChunkEntry<T>(Vec<T>);

impl<T> RepeatedChunkEntry<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn add_value(&mut self, _id: u32, _name: &str, value: T) -> Result<(), ChunkError> {
        self.0.push(value);
        Ok(())
    }

    pub fn finish(self, _id: u32, _name: &str) -> Result<Vec<T>, ChunkError> {
        Ok(self.0)
    }
}

#[doc(hidden)]
pub fn unexpected_chunk(id: u32) -> ChunkError {
    ChunkError::FormatError(ChunkMapFormatError::UnexpectedChunk { id }.into())
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
