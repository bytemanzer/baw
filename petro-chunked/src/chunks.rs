use crate::ChunkError;
use crate::byteblock::{FromByteBlock, FromFixedByteBlock, TakeBytes};
use crate::content::DataOrMiniChunks;
use crate::error::InsufficientDataHelper;

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
    pub fn data(self) -> Option<DataOrMiniChunks<'b>> {
        match self {
            Self::Data(data) => Some(data),
            _ => None,
        }
    }

    /// If the chunk content is more chunks, returns an iterator over the child chunks.
    pub fn children(self) -> Option<ChunksIter<'b>> {
        match self {
            Self::Chunks(chunks) => Some(chunks),
            _ => None,
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
    /// more chunks, return None.
    pub fn data(&self) -> Option<DataOrMiniChunks<'b>> {
        self.content().data()
    }

    /// If the chunk contains child chunks but not mini chunks, returns an iterator over the
    /// children.
    pub fn children(&self) -> Option<ChunksIter<'b>> {
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
    type Error = ChunkError;
    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), Self::Error> {
        let (id, LenKind { len, kind }) = bytes.take().err_to_header()?;
        let content = bytes.take_bytes(len).err_to_data()?;
        let chunk = Chunk { id, kind, content };
        Ok((chunk, bytes))
    }
}

impl<'b> FromByteBlock<'b> for Option<Chunk<'b>> {
    type Error = ChunkError;
    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), Self::Error> {
        if bytes.is_empty() {
            return Ok((None, bytes))
        }
        let (id, LenKind { len, kind }) = bytes.take().err_to_header()?;
        let content = bytes.take_bytes(len).err_to_data()?;
        let chunk = Chunk { id, kind, content };
        Ok((Some(chunk), bytes))
    }
}
