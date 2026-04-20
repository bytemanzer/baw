use thiserror::Error;

use crate::byteblock::NotEnoughData;

/// Error results from chunk parsing.
#[derive(Error, Debug, Clone)]
pub enum ChunkError {
    /// There were not enough bytes to read the chunk header.
    #[error(
        "Not enough bytes to read the chunk header. Expected at least {expected} bytes for the \
        header, but only {available} were available."
    )]
    IncompleteHeader {
        /// Number of bytes needed for the chunk header.
        expected: usize,
        /// The number of bytes that were available.
        available: usize,
    },
    /// The chunk or its parent did not have as much data as the content of this chunk requires.
    #[error(
        "Not enough bytes to read the chunk content. Expected at least {expected} bytes for the \
        chunk body, but only {available} were available."
    )]
    DataTruncated {
        /// The number of bytes according to the chunk size.
        expected: usize,
        /// The number of bytes that were available after the chunk header.
        available: usize,
    },
}

pub(crate) trait InsufficientDataHelper {
    type Converted;

    fn err_to_header(self) -> Self::Converted;

    fn err_to_data(self) -> Self::Converted;
}

impl InsufficientDataHelper for NotEnoughData {
    type Converted = ChunkError;

    fn err_to_header(self) -> Self::Converted {
        ChunkError::IncompleteHeader {
            expected: self.requested,
            available: self.available,
        }
    }

    fn err_to_data(self) -> Self::Converted {
        ChunkError::DataTruncated {
            expected: self.requested,
            available: self.available,
        }
    }
}

impl<T> InsufficientDataHelper for Result<T, NotEnoughData> {
    type Converted = Result<T, ChunkError>;

    fn err_to_header(self) -> Self::Converted {
        self.map_err(InsufficientDataHelper::err_to_header)
    }

    fn err_to_data(self) -> Self::Converted {
        self.map_err(InsufficientDataHelper::err_to_data)
    }
}
