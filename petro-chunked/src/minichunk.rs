use crate::ChunkError;
use crate::byteblock::{FromByteBlock, TakeBytes};
use crate::content::{ReadContent, ReadContentError};
use crate::error::InsufficientDataHelper;

/// Interprets the given bytes as containing mini-chunks and gets an iterator over them.
pub fn iter_mini_chunks<'b>(data: &'b [u8]) -> MiniChunksIter<'b> {
    MiniChunksIter { data }
}

/// Iterator over [`MiniChunk`]s in some source data.
pub struct MiniChunksIter<'b> {
    /// Source bytes to extract chunks from.
    data: &'b [u8],
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

impl<'b> ReadContent<'b> for MiniChunk<'b> {
    fn read<T: FromByteBlock<'b>>(&self) -> Result<T, ReadContentError<T::Error>> {
        self.content.read()
    }
}

impl<'b> FromByteBlock<'b> for MiniChunk<'b> {
    type Error = ChunkError;

    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), Self::Error> {
        let (id, len): (u8, u8) = bytes.take().err_to_header()?;
        let content = bytes.take_bytes(len as usize).err_to_data()?;
        let chunk = MiniChunk { id, content };
        Ok((chunk, bytes))
    }
}

impl<'b> FromByteBlock<'b> for Option<MiniChunk<'b>> {
    type Error = ChunkError;

    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), Self::Error> {
        if bytes.is_empty() {
            return Ok((None, bytes));
        }
        let (id, len): (u8, u8) = bytes.take().err_to_header()?;
        let content = bytes.take_bytes(len as usize).err_to_data()?;
        let chunk = MiniChunk { id, content };
        Ok((Some(chunk), bytes))
    }
}
