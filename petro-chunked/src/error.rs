use std::borrow::Cow;
use std::error::Error;
use std::ffi::FromBytesUntilNulError;
use std::fmt;
use std::str::Utf8Error;

use thiserror::Error;

/// Identifies the general category of a [`ChunkErrorV2`].
///
/// Currently this mainly differentiates strucutral issues (chunk order, number, presence, etc.)
/// from truncation/leftovers, and values of invalid format.
#[derive(Error, Debug, Copy, Clone, PartialEq, Eq)]
pub enum ChunkErrorKind {
    /// The chunk or mini chunk sequence was not in the expected format. This may indicate a
    /// required chunk was missing, chunks were not in the expected order, extra unrecognized chunks
    /// were found, etc. It may also indicate that a chunk contained data/mini-chunks when it was
    /// expected to contain child chunks, or vice-versa.
    #[error("InvalidChunkStructure")]
    InvalidChunkStructure,
    /// A data block was the wrong length. This could be the contents of a chunk or mini chunk, or
    /// the overall file. Something had a length that was different from what was expected. It might
    /// mean there were too few bytes, or that there were bytes left over after parsing something
    /// that was supposed to consume all of the available bytes.
    #[error("WrongDataLength")]
    WrongDataLength,
    /// The contents of some portion of the data did not have a valid value. For example, a boolean
    /// value which had a value other than 0 or 1.
    #[error("InvalidValue")]
    InvalidValue,
}

/// Holds context for a [`ChunkErrorV2`].
///
/// This is a struct rather than a plain Cow/String because we expect to add byte position
/// information to these later.
#[derive(Debug)]
struct ContextData {
    /// The message for an error context. For most derive-generated errors, this is an `&'static
    /// str` stating the thing which the parser was trying to parse at the time. However in some
    /// cases, especially the root `message` of the error, we do expect to accept formatted strings
    /// describing the exact "wrong" value that was received.
    message: Cow<'static, str>,
}

impl ContextData {
    /// Construct a ChunkErrorContext with the given message.
    fn new<M: Into<Cow<'static, str>>>(message: M) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ContextData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Inner contents of a [ChunkErrorV2], used to ensure that [`ChunkErrorV2`] remains small (pointer
/// sized).
struct ChunkErrorInner {
    /// General category of this chunk error. Every ChunkError contains at least this category.
    kind: ChunkErrorKind,
    /// An optional message for the "root" of the error. This is separate from `contexts`, since it
    /// will be rendered next to the `kind` rather than on its own line.
    message: Option<ContextData>,
    /// Additional contexts applied to the [`ChunkErrorV2`]. These are listed in undwind order, so
    /// the outermost call is at the bottom. This is similar to `absl::Status/StatusOr`, which puts
    /// the original cause at the top, contra `anyhow::Error`, which puts the most recent context
    /// message first.
    context: Vec<ContextData>,
}

/// Helper macro that constructs a boxed [`ChunkErrorInner`] wrapped in [`ChunkErrorV2`] using
/// struct syntax.
macro_rules! ChunkErrorV2 {
    ($($args:tt)*) => {
        ChunkErrorV2(Box::new(ChunkErrorInner {
            $($args)*
        }))
    }
}

/// Error produced by structured chunk parsing.
///
/// Contains a [`ChunkErrorKind`] which refers to the general category of error that caused the
/// parse error, and an optional message describing the root cause of the error.
///
/// Additionally contains an optional stack of contexts, which describe the outer layers of the call
/// stack where the error occured. These are ordered from innermost to outermost, similar to
/// `absl::Status`/`absl::StatusError`, which is the opposite of how `anyhow::Error` handles
/// contexts.
// We could do some hacky pointer inliney thing if we expected to often have errors that were *just*
// a ChunkErrorKind, but for the most part, we actually expect to have explicit messages, since the
// derive generator will emit them.
pub struct ChunkErrorV2(Box<ChunkErrorInner>);

impl ChunkErrorV2 {
    /// Creates a new [`ChunkErrorV2`] with the given kind and root cause error.
    pub fn new<M: Into<Cow<'static, str>>>(kind: ChunkErrorKind, message: M) -> Self {
        ChunkErrorV2! {
            kind,
            message: Some(ContextData::new(message)),
            context: Vec::new(),
        }
    }

    /// Gets the error kind.
    pub fn kind(&self) -> ChunkErrorKind {
        self.0.kind
    }

    /// Gets the root-cause error message.
    pub fn message(&self) -> Option<&str> {
        self.0.message.as_ref().map(|m| m.message.as_ref())
    }

    /// Adds an additional, outer context message to the end of the context stack.
    pub fn context<M: Into<Cow<'static, str>>>(mut self, message: M) -> Self {
        self.0.context.push(ContextData::new(message));
        self
    }
}

/// Creates a [`ChunkErrorV2`] with no message.
impl From<ChunkErrorKind> for ChunkErrorV2 {
    fn from(kind: ChunkErrorKind) -> Self {
        ChunkErrorV2! {
            kind,
            message: None,
            context: Vec::new(),
        }
    }
}

impl fmt::Debug for ChunkErrorV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChunkError")
            .field("kind", &self.0.kind)
            .field("message", &self.0.message)
            .field("context", &self.0.context)
            .finish()
    }
}

impl fmt::Display for ChunkErrorV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error {}", self.kind())?;
        if let Some(ref message) = self.0.message {
            write!(f, ": {message}")?;
        }
        if f.alternate() {
            for (idx, ctx) in self.0.context.iter().enumerate() {
                let line = if idx == self.0.context.len() - 1 {
                    '\u{2570}'
                } else {
                    '\u{251c}'
                };
                write!(f, "\n    {line} {ctx}")?;
            }
        }
        Ok(())
    }
}

impl Error for ChunkErrorV2 {}

impl From<ChunkError> for ChunkErrorV2 {
    fn from(err: ChunkError) -> Self {
        let message = err.to_string();
        match err {
            ChunkError::HeaderError(source)
            | ChunkError::MiniHeaderError(source)
            | ChunkError::DataError { source, .. }
            | ChunkError::MiniChunkDataError { source, .. } => {
                ChunkErrorV2::from(*source).context(message)
            }
            ChunkError::NotEnoughData { .. } | ChunkError::UnusedData { .. } => {
                ChunkErrorV2::new(ChunkErrorKind::WrongDataLength, message)
            }
            ChunkError::FormatError(err) => match err.downcast::<ChunkErrorV2>() {
                // This is the one spot where pre-computing message is a waste, but it's not worth
                // trying to avoid it.
                Ok(v2) => v2,
                Err(_) => Self::new(ChunkErrorKind::InvalidValue, message),
            },
            ChunkError::UnexpectedChunk { .. }
            | ChunkError::EndOfChunks
            | ChunkError::ExpectedChunks
            | ChunkError::ExpectedData => {
                ChunkErrorV2::new(ChunkErrorKind::InvalidChunkStructure, message)
            }
        }
    }
}

impl From<ChunkErrorV2> for ChunkError {
    fn from(err: ChunkErrorV2) -> Self {
        // ChunkErrorV2 is not specific enough to determine anything more. But that's OK becuause we
        // intend to deprecate and remove it!
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

trait SealedV2 {}

/// Helper to support chunk error context on `Result<T, ChunkErrorV2>`
#[allow(private_bounds)]
pub trait ChunkErrorContextV2<T>: SealedV2 {
    /// Adds chunk error context with the given message to the error, if it is in the error state.
    ///
    /// Named to avoid collision with `anyhow::Context`.
    fn chunk_context<M: Into<Cow<'static, str>>>(self, message: M) -> Result<T, ChunkErrorV2>;
}

impl<T, E> SealedV2 for Result<T, E> where E: Into<ChunkErrorV2> {}

impl<T, E> ChunkErrorContextV2<T> for Result<T, E>
where
    E: Into<ChunkErrorV2>,
{
    fn chunk_context<M: Into<Cow<'static, str>>>(self, message: M) -> Result<T, ChunkErrorV2> {
        self.map_err(|e| e.into().context(message))
    }
}
