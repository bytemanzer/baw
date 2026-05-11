use petro_chunked::byteblock::FromByteBlock;

use crate::error::FormatError;

/// Provides unpacking boolean values from u32.
pub(crate) struct U32Bool {
    pub value: bool,
}

impl<'b> FromByteBlock<'b> for U32Bool {
    fn take_from(bytes: &'b [u8]) -> Result<(Self, &'b [u8]), petro_chunked::ChunkError> {
        let (value, rest) = u32::take_from(bytes)?;
        let value = match value {
            0 => false,
            1 => true,
            _ => Err(FormatError::UnexpectedBool { value })?,
        };
        Ok((U32Bool { value }, rest))
    }
}

/// Provides unpacking usize values from u32.
pub(crate) struct U32Usize {
    pub value: usize,
}

impl<'b> FromByteBlock<'b> for U32Usize {
    fn take_from(bytes: &'b [u8]) -> Result<(Self, &'b [u8]), petro_chunked::ChunkError> {
        let (value, rest) = u32::take_from(bytes)?;
        let value = value.try_into().map_err(anyhow::Error::new)?;
        Ok((U32Usize { value }, rest))
    }
}
