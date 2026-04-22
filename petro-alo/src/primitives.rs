use std::ffi::CStr;

use petro_chunked::byteblock::FromByteBlock;
use petro_chunked::chunks::ChunksIter;

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

/// Look for a chunk with the given ID as the first chunk in the iterator, and interpret it
/// as a string.
pub(crate) fn expect_string_chunk(chunks: &mut ChunksIter, id: u32) -> anyhow::Result<String> {
    Ok(expect_str_chunk(chunks, id)?.to_owned())
}

/// Look for a chunk with the given ID as the first chunk in the iterator, and interpret it
/// as a borrowed string.
pub(crate) fn expect_str_chunk<'b>(
    chunks: &mut ChunksIter<'b>,
    id: u32,
) -> anyhow::Result<&'b str> {
    let string = chunks.expect_chunk(id)?.data()?.as_data();
    let string = CStr::from_bytes_until_nul(string)?.to_str()?;
    Ok(string)
}
