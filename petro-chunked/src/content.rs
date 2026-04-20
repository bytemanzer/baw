//! Utilites for reading chunk contents.
use std::ops::Deref;

use thiserror::Error;

use crate::byteblock::{FromByteBlock, TakeBytes};
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

impl<'b> ReadContent<'b> for DataOrMiniChunks<'b> {
    fn read<T: FromByteBlock<'b>>(&self) -> Result<T, ReadContentError<T::Error>> {
        self.data.read()
    }
}

impl<'b> ReadContent<'b> for &'b [u8] {
    fn read<T: FromByteBlock<'b>>(&self) -> Result<T, ReadContentError<T::Error>>
    where
        T::Error: Into<ReadContentError<T::Error>>,
    {
        let mut data = *self;
        let result = data.take()?;
        if data.is_empty() {
            Ok(result)
        } else {
            Err(ReadContentError::UnusedData { len: data.len() })
        }
    }
}

/// Trait for reading the contents of a chunk's data or a mini-chunk as a fixed size type.
pub trait ReadContent<'b> {
    /// Read the content as the given byte block type.
    fn read<T: FromByteBlock<'b>>(&self) -> Result<T, ReadContentError<T::Error>>;

    /// Utility to map the result of taking some data, where the type to take can be inferred from
    /// the function's argument type.
    fn read_map<F, T, U>(&self, func: F) -> Result<U, ReadContentError<T::Error>>
    where
        F: FnOnce(T) -> U,
        T: FromByteBlock<'b>,
    {
        self.read::<T>().map(func)
    }
}

/// Error when reading chunk content as a fixed-size type.
#[derive(Error, Debug)]
pub enum ReadContentError<T> {
    #[error("Failed to take contents")]
    TakeError(#[from] T),
    #[error("{len} bytes of data was left over after reading")]
    UnusedData { len: usize },
}
