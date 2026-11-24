//! Wire format adapter types for use with `wire_format = <Type>` in derive attributes.

use std::num::TryFromIntError;

use crate::byteblock::{FromByteBlock, TakeBytes};
use crate::error::{ChunkError, ChunkErrorKind, ChunkErrorV2};

/// Wire format type for a boolean stored as a single byte (`0` or `1`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct U8Bool(pub bool);

impl From<U8Bool> for bool {
    fn from(value: U8Bool) -> Self {
        value.0
    }
}

impl From<bool> for U8Bool {
    fn from(value: bool) -> Self {
        Self(value)
    }
}

impl<'b> FromByteBlock<'b> for U8Bool {
    fn take_from(bytes: &'b [u8]) -> Result<(Self, &'b [u8]), ChunkError> {
        let (value, rest) = u8::take_from(bytes)?;
        let value = match value {
            0 => false,
            1 => true,
            raw => {
                return Err(ChunkErrorV2::new(
                    ChunkErrorKind::InvalidValue,
                    format!("Expected a boolean value (0 or 1), but found {raw}"),
                )
                .into());
            }
        };
        Ok((Self(value), rest))
    }
}

/// Wire format type for a boolean stored as a 32-bit little-endian integer (`0` or `1`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct U32Bool(pub bool);

impl From<U32Bool> for bool {
    fn from(value: U32Bool) -> Self {
        value.0
    }
}

impl From<bool> for U32Bool {
    fn from(value: bool) -> Self {
        Self(value)
    }
}

impl<'b> FromByteBlock<'b> for U32Bool {
    fn take_from(bytes: &'b [u8]) -> Result<(Self, &'b [u8]), ChunkError> {
        let (value, rest) = u32::take_from(bytes)?;
        let value = match value {
            0 => false,
            1 => true,
            raw => {
                return Err(ChunkErrorV2::new(
                    ChunkErrorKind::InvalidValue,
                    format!("Expected a boolean value (0 or 1), but found {raw}"),
                )
                .into());
            }
        };
        Ok((Self(value), rest))
    }
}

/// Wire format type for a `usize` stored as a 32-bit little-endian unsigned integer (`u32`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct U32Usize(pub usize);

impl From<U32Usize> for usize {
    fn from(value: U32Usize) -> Self {
        value.0
    }
}

impl From<usize> for U32Usize {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl<'b> FromByteBlock<'b> for U32Usize {
    fn take_from(bytes: &'b [u8]) -> Result<(Self, &'b [u8]), ChunkError> {
        let (value, rest) = u32::take_from(bytes)?;
        let value = value.try_into().map_err(|e: TryFromIntError| {
            ChunkErrorV2::new(ChunkErrorKind::InvalidValue, e.to_string())
        })?;
        Ok((Self(value), rest))
    }
}

/// Wire format type for a `Vec<usize>` stored as a packed sequence of `u32` values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct U32UsizeVec(pub Vec<usize>);

impl From<U32UsizeVec> for Vec<usize> {
    fn from(value: U32UsizeVec) -> Self {
        value.0
    }
}

impl From<Vec<usize>> for U32UsizeVec {
    fn from(value: Vec<usize>) -> Self {
        Self(value)
    }
}

impl<'b> FromByteBlock<'b> for U32UsizeVec {
    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), ChunkError> {
        let value = bytes
            .take_iter::<U32Usize>()
            .map(|res| res.map(|U32Usize(v)| v))
            .collect::<Result<Vec<usize>, _>>()?;
        Ok((Self(value), bytes))
    }
}
