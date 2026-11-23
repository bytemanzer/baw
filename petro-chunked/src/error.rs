use std::ffi::FromBytesUntilNulError;
use std::fmt;
use std::str::Utf8Error;

use thiserror::Error;

/// High-level category of a [`ChunkErrorV2`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ChunkErrorKind {
    NotEnoughData,
    UnusedData,
    UnexpectedChunk,
    DuplicateChunk,
    MissingRequiredChunk,
    EndOfChunks,
    ExpectedChunks,
    ExpectedData,
    InvalidValue,
}

impl fmt::Display for ChunkErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::NotEnoughData => "NotEnoughData",
            Self::UnusedData => "UnusedData",
            Self::UnexpectedChunk => "UnexpectedChunk",
            Self::DuplicateChunk => "DuplicateChunk",
            Self::MissingRequiredChunk => "MissingRequiredChunk",
            Self::EndOfChunks => "EndOfChunks",
            Self::ExpectedChunks => "ExpectedChunks",
            Self::ExpectedData => "ExpectedData",
            Self::InvalidValue => "InvalidValue",
        };
        f.write_str(s)
    }
}

type BoxedStdError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Structured chunk parsing error with a pinned [`ChunkErrorKind`], root cause, and stacked
/// context layers rendered in reverse nesting order.
#[derive(Debug)]
pub struct ChunkErrorV2 {
    kind: ChunkErrorKind,
    source: BoxedStdError,
    contexts: Vec<BoxedStdError>,
}

impl ChunkErrorV2 {
    /// Creates a new [`ChunkErrorV2`] with the given kind and root cause error.
    pub fn new<E>(kind: ChunkErrorKind, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind,
            source: Box::new(source),
            contexts: Vec::new(),
        }
    }

    /// Returns the error category.
    pub fn kind(&self) -> ChunkErrorKind {
        self.kind
    }

    /// Returns the stacked context errors in reverse nesting order (innermost to outermost).
    pub fn contexts(&self) -> &[BoxedStdError] {
        &self.contexts
    }

    /// Pushes an outer context error onto this error.
    pub fn with_context<C>(mut self, context: C) -> Self
    where
        C: std::error::Error + Send + Sync + 'static,
    {
        self.contexts.push(Box::new(context));
        self
    }

    /// Adds a context indicating the error occurred while reading a chunk header.
    pub fn err_to_header(self) -> Self {
        self.with_context(ChunkHeaderContext)
    }

    /// Adds a context indicating the error occurred while reading a mini-chunk header.
    pub fn err_to_mini_header(self) -> Self {
        self.with_context(MiniChunkHeaderContext)
    }

    /// Adds a context indicating the error occurred while reading the contents of chunk `id`.
    pub fn err_to_data(self, id: u32) -> Self {
        self.with_context(ChunkDataContext { id })
    }

    /// Adds a context indicating the error occurred while reading the contents of mini-chunk `id`.
    pub fn err_to_mini_chunk_data(self, id: u8) -> Self {
        self.with_context(MiniChunkDataContext { id })
    }
}

impl fmt::Display for ChunkErrorV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error {}: {}", self.kind, self.source)?;
        for ctx in &self.contexts {
            write!(f, "\n    - {ctx}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ChunkErrorV2 {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Error, Debug, Copy, Clone)]
#[error("while parsing chunk header")]
pub struct ChunkHeaderContext;

#[derive(Error, Debug, Copy, Clone)]
#[error("while parsing mini-chunk header")]
pub struct MiniChunkHeaderContext;

#[derive(Error, Debug, Copy, Clone)]
#[error("while parsing chunk {id:#010x}")]
pub struct ChunkDataContext {
    pub id: u32,
}

#[derive(Error, Debug, Copy, Clone)]
#[error("while parsing mini-chunk {id:#04x}")]
pub struct MiniChunkDataContext {
    pub id: u8,
}

#[derive(Error, Debug, Copy, Clone)]
#[error(
    "Not enough data to take the requested number of bytes. Requested: {requested}, Available: {available}"
)]
pub struct NotEnoughDataError {
    pub requested: usize,
    pub available: usize,
}

#[derive(Error, Debug, Copy, Clone)]
#[error("Expected to use the entire remaining data, but {len} bytes were left over")]
pub struct UnusedDataError {
    pub len: usize,
}

#[derive(Error, Debug, Copy, Clone)]
#[error("Unexpected chunk: {id:0>8x}")]
pub struct UnexpectedChunkError {
    pub id: u32,
}

#[derive(Error, Debug, Copy, Clone)]
#[error("Expected additional chunks but hit end of chunks")]
pub struct EndOfChunksError;

#[derive(Error, Debug, Copy, Clone)]
#[error("Expected to find child chunks, but chunk contained data")]
pub struct ExpectedChunksError;

#[derive(Error, Debug, Copy, Clone)]
#[error("Expected to find data, but chunk contained child chunks")]
pub struct ExpectedDataError;

#[derive(Debug)]
struct AnyhowErrorAdapter(anyhow::Error);

impl fmt::Display for AnyhowErrorAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for AnyhowErrorAdapter {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl From<ChunkError> for ChunkErrorV2 {
    fn from(err: ChunkError) -> Self {
        match err {
            ChunkError::HeaderError(source) => Self::from(*source).err_to_header(),
            ChunkError::MiniHeaderError(source) => Self::from(*source).err_to_mini_header(),
            ChunkError::DataError { chunk, source } => Self::from(*source).err_to_data(chunk),
            ChunkError::MiniChunkDataError { chunk, source } => {
                Self::from(*source).err_to_mini_chunk_data(chunk)
            }
            ChunkError::NotEnoughData {
                requested,
                available,
            } => Self::new(
                ChunkErrorKind::NotEnoughData,
                NotEnoughDataError {
                    requested,
                    available,
                },
            ),
            ChunkError::FormatError(err) => {
                match err.downcast::<ChunkErrorV2>() {
                    Ok(v2) => v2,
                    Err(err) => Self::new(ChunkErrorKind::InvalidValue, AnyhowErrorAdapter(err)),
                }
            }
            ChunkError::UnusedData { len } => {
                Self::new(ChunkErrorKind::UnusedData, UnusedDataError { len })
            }
            ChunkError::UnexpectedChunk { id } => {
                Self::new(ChunkErrorKind::UnexpectedChunk, UnexpectedChunkError { id })
            }
            ChunkError::EndOfChunks => Self::new(ChunkErrorKind::EndOfChunks, EndOfChunksError),
            ChunkError::ExpectedChunks => {
                Self::new(ChunkErrorKind::ExpectedChunks, ExpectedChunksError)
            }
            ChunkError::ExpectedData => Self::new(ChunkErrorKind::ExpectedData, ExpectedDataError),
        }
    }
}

impl From<ChunkErrorV2> for ChunkError {
    fn from(err: ChunkErrorV2) -> Self {
        if err.contexts.is_empty() {
            match err.kind {
                ChunkErrorKind::NotEnoughData => {
                    if let Some(e) = err.source.downcast_ref::<NotEnoughDataError>() {
                        return Self::NotEnoughData {
                            requested: e.requested,
                            available: e.available,
                        };
                    }
                }
                ChunkErrorKind::UnusedData => {
                    if let Some(e) = err.source.downcast_ref::<UnusedDataError>() {
                        return Self::UnusedData { len: e.len };
                    }
                }
                ChunkErrorKind::UnexpectedChunk => {
                    if let Some(e) = err.source.downcast_ref::<UnexpectedChunkError>() {
                        return Self::UnexpectedChunk { id: e.id };
                    }
                }
                ChunkErrorKind::EndOfChunks => return Self::EndOfChunks,
                ChunkErrorKind::ExpectedChunks => return Self::ExpectedChunks,
                ChunkErrorKind::ExpectedData => return Self::ExpectedData,
                _ => {}
            }
        }
        Self::FormatError(anyhow::Error::new(err))
    }
}

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
