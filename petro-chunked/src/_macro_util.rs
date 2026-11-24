//! Internal helpers used by `petro-chunked-derive` and `mini_chunk_map!`.

use thiserror::Error;

use crate::error::{ChunkError, ChunkErrorKind, ChunkErrorV2, UnexpectedChunkError};

#[derive(Error, Debug)]
#[error("Chunk {id:0>8x} ({name}) appeared more than once")]
pub struct DuplicateChunkError {
    pub id: u32,
    pub name: &'static str,
}

#[derive(Error, Debug)]
#[error("Chunk {id:0>8x} ({name}) was required but not found")]
pub struct MissingRequiredChunkError {
    pub id: u32,
    pub name: &'static str,
}

/// Collection helper for required map-mode chunks.
pub struct RequiredChunkEntry<T>(Option<T>);

impl<T> RequiredChunkEntry<T> {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn add_value(&mut self, id: u32, name: &'static str, value: T) -> Result<(), ChunkErrorV2> {
        if self.0.is_none() {
            self.0 = Some(value);
            Ok(())
        } else {
            Err(ChunkErrorV2::new(
                ChunkErrorKind::DuplicateChunk,
                DuplicateChunkError { id, name },
            ))
        }
    }

    pub fn finish(self, id: u32, name: &'static str) -> Result<T, ChunkErrorV2> {
        match self.0 {
            Some(val) => Ok(val),
            None => Err(ChunkErrorV2::new(
                ChunkErrorKind::MissingRequiredChunk,
                MissingRequiredChunkError { id, name },
            )),
        }
    }
}

/// Collection helper for optional map-mode chunks.
pub struct OptionalChunkEntry<T>(Option<T>);

impl<T> OptionalChunkEntry<T> {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn add_value(&mut self, id: u32, name: &'static str, value: T) -> Result<(), ChunkErrorV2> {
        if self.0.is_none() {
            self.0 = Some(value);
            Ok(())
        } else {
            Err(ChunkErrorV2::new(
                ChunkErrorKind::DuplicateChunk,
                DuplicateChunkError { id, name },
            ))
        }
    }

    pub fn finish(self, _id: u32, _name: &'static str) -> Result<Option<T>, ChunkErrorV2> {
        Ok(self.0)
    }

    pub fn finish_or_default(self) -> Result<T, ChunkErrorV2>
    where
        T: Default,
    {
        Ok(self.0.unwrap_or_default())
    }

    pub fn finish_or_else(self, f: impl FnOnce() -> T) -> Result<T, ChunkErrorV2> {
        Ok(self.0.unwrap_or_else(f))
    }
}

/// Collection helper for repeated map-mode chunks.
pub struct RepeatedChunkEntry<T>(Vec<T>);

impl<T> RepeatedChunkEntry<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn add_value(
        &mut self,
        _id: u32,
        _name: &'static str,
        value: T,
    ) -> Result<(), ChunkErrorV2> {
        self.0.push(value);
        Ok(())
    }

    pub fn finish(self, _id: u32, _name: &'static str) -> Result<Vec<T>, ChunkErrorV2> {
        Ok(self.0)
    }
}

pub fn unexpected_chunk(id: u32) -> ChunkErrorV2 {
    ChunkErrorV2::new(
        ChunkErrorKind::UnexpectedChunk,
        UnexpectedChunkError { id },
    )
}

#[derive(Error, Debug)]
#[error("Found unexpected MiniChunk {id:0>2x}")]
pub struct UnexpectedMiniChunkError {
    pub id: u8,
}

#[derive(Error, Debug)]
#[error("MiniChunk {id:0>2x} ({name}) appeared more than once")]
pub struct DuplicateMiniChunkError {
    pub id: u8,
    pub name: &'static str,
}

#[derive(Error, Debug)]
#[error("MiniChunk {id:0>2x} ({name}) was required but not found")]
pub struct MissingRequiredMiniChunkError {
    pub id: u8,
    pub name: &'static str,
}

/// Collection helper for required map-mode mini-chunks.
pub struct RequiredMapEntry<T>(Option<T>);

impl<T> RequiredMapEntry<T> {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn add_value(&mut self, id: u8, name: &'static str, value: T) -> Result<(), ChunkError> {
        if self.0.is_none() {
            self.0 = Some(value);
            Ok(())
        } else {
            Err(ChunkErrorV2::new(
                ChunkErrorKind::DuplicateChunk,
                DuplicateMiniChunkError { id, name },
            )
            .into())
        }
    }

    pub fn finish(self, id: u8, name: &'static str) -> Result<T, ChunkError> {
        match self.0 {
            Some(val) => Ok(val),
            None => Err(ChunkErrorV2::new(
                ChunkErrorKind::MissingRequiredChunk,
                MissingRequiredMiniChunkError { id, name },
            )
            .into()),
        }
    }
}

/// Collection helper for optional map-mode mini-chunks.
pub struct OptionalMapEntry<T>(Option<T>);

impl<T> OptionalMapEntry<T> {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn add_value(&mut self, id: u8, name: &'static str, value: T) -> Result<(), ChunkError> {
        if self.0.is_none() {
            self.0 = Some(value);
            Ok(())
        } else {
            Err(ChunkErrorV2::new(
                ChunkErrorKind::DuplicateChunk,
                DuplicateMiniChunkError { id, name },
            )
            .into())
        }
    }

    pub fn finish(self, _id: u8, _name: &str) -> Result<Option<T>, ChunkError> {
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

/// Collection helper for repeated map-mode mini-chunks.
pub struct RepeatedMapEntry<T>(Vec<T>);

impl<T> RepeatedMapEntry<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn add_value(&mut self, _id: u8, _name: &str, value: T) -> Result<(), ChunkError> {
        self.0.push(value);
        Ok(())
    }

    pub fn finish(self, _id: u8, _name: &str) -> Result<Vec<T>, ChunkError> {
        Ok(self.0)
    }
}

pub fn unexpected_mini_chunk(id: u8) -> ChunkError {
    ChunkErrorV2::new(
        ChunkErrorKind::UnexpectedChunk,
        UnexpectedMiniChunkError { id },
    )
    .into()
}
