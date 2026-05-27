use std::ffi::FromBytesUntilNulError;
use std::str::Utf8Error;

use thiserror::Error;

/// Error results from chunk parsing.
#[derive(Error, Debug)]
pub enum ChunkError {
    #[error("Error reading chunk header: {0}")]
    HeaderError(#[source] Box<ChunkError>),
    #[error("Error reading mini-chunk header: {0}")]
    MiniHeaderError(#[source] Box<ChunkError>),
    /// The chunk or its parent did not have as much data as the content of this chunk requires.
    #[error("Error reading contents of chunk {chunk:0>8x}: {source}")]
    DataError {
        /// ID of the currently processing chunk.
        chunk: u32,
        /// Error that caused this error.
        source: Box<ChunkError>,
    },
    #[error("Error reading contents of mini-chunk {chunk:0>8x}: {source}")]
    MiniChunkDataError {
        /// ID of the currently processing mini-chunk.
        chunk: u8,
        /// Error that caused this error.
        source: Box<ChunkError>,
    },
    #[error(
        "Not enough data to take the requested number of bytes. Requested: {requested}, Available: \
        {available}"
    )]
    NotEnoughData {
        /// Number of bytes requested to read.
        requested: usize,
        /// Number of bytes that were available to read.
        available: usize,
    },
    #[error("Contents did not have the expected format: {0}")]
    FormatError(#[from] anyhow::Error),
    #[error("Expected to use the entire remaining data, but {len} bytes were left over")]
    UnusedData { len: usize },
    #[error("Unexpected chunk: {id:0>8x}")]
    UnexpectedChunk { id: u32 },
    #[error("Expected additional chunks but hit end of chunks")]
    EndOfChunks,
    #[error("Expected to find child chunks, but chunk contained data")]
    ExpectedChunks,
    #[error("Expected to find data, but chunk contained child chunks")]
    ExpectedData,
}

impl ChunkError {
    /// Re-wraps a ChunkError to indicate that it happened while processing the header.
    pub fn err_to_header(self) -> Self {
        Self::HeaderError(Box::new(self))
    }

    /// Re-wraps a ChunkError to indicate that it happened while processing a mini-chunk header.
    pub fn err_to_mini_header(self) -> Self {
        Self::MiniHeaderError(Box::new(self))
    }

    /// Re-wraps a ChunkError to indicate that it happened while processing the data of the chunk
    /// with the given ID.
    pub fn err_to_data(self, id: u32) -> Self {
        Self::DataError {
            chunk: id,
            source: Box::new(self),
        }
    }

    /// Re-wraps a ChunkError to indicate that it happened while processing the data of the
    /// mini-chunk with the given ID.
    pub fn err_to_mini_chunk_data(self, id: u8) -> Self {
        Self::MiniChunkDataError {
            chunk: id,
            source: Box::new(self),
        }
    }
}

impl From<FromBytesUntilNulError> for ChunkError {
    fn from(value: FromBytesUntilNulError) -> Self {
        Self::FormatError(value.into())
    }
}

impl From<Utf8Error> for ChunkError {
    fn from(value: Utf8Error) -> Self {
        Self::FormatError(value.into())
    }
}

trait Sealed {}

/// Helper to support chunk error methods on Result<T, ChunkError>
#[allow(private_bounds)]
pub trait ChunkErrorContext<T>: Sealed {
    /// Re-wraps a ChunkError to indicate that it happened while processing the header.
    fn err_to_header(self) -> Result<T, ChunkError>;

    /// Re-wraps a ChunkError to indicate that it happened while processing a mini-chunk header.
    fn err_to_mini_header(self) -> Result<T, ChunkError>;

    /// Re-wraps a ChunkError to indicate that it happened while processing the data of the chunk
    /// with the given ID.
    fn err_to_data(self, id: u32) -> Result<T, ChunkError>;

    /// Re-wraps a ChunkError to indicate that it happened while processing the data of the
    /// mini-chunk with the given ID.
    fn err_to_mini_chunk_data(self, id: u8) -> Result<T, ChunkError>;
}

impl<T, E> Sealed for Result<T, E> where E: Into<ChunkError> {}

impl<T, E> ChunkErrorContext<T> for Result<T, E>
where
    E: Into<ChunkError>,
{
    fn err_to_header(self) -> Result<T, ChunkError> {
        self.map_err(|e| e.into().err_to_header())
    }

    fn err_to_mini_header(self) -> Result<T, ChunkError> {
        self.map_err(|e| e.into().err_to_mini_header())
    }

    fn err_to_data(self, id: u32) -> Result<T, ChunkError> {
        self.map_err(|e| e.into().err_to_data(id))
    }

    fn err_to_mini_chunk_data(self, id: u8) -> Result<T, ChunkError> {
        self.map_err(|e| e.into().err_to_mini_chunk_data(id))
    }
}
