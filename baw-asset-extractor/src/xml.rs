//! Provides helpers for dealing with encoding jank in xml files.

use std::fmt;
use std::io::Read;

use anyhow::bail;
use encoding_rs::DecoderResult;
use petro_meg::path::{MegPath, MegPathBuf};
use serde::de::{DeserializeOwned, Error as DeError, Visitor};
use serde::{Deserialize, Deserializer};
use tracing::warn;

/// Deserialize the entire file as XML, by first loading it into memory and decoding it as either
/// utf-8 or CP 1252.
pub fn deserialize_xml_utf8_or_cp1252<T: DeserializeOwned, R: Read>(mut r: R) -> anyhow::Result<T> {
    let mut buffer = Vec::new();
    r.read_to_end(&mut buffer)?;
    let contents = decode_utf8_or_cp1252(buffer)?;
    Ok(quick_xml::de::from_str(&contents)?)
}

/// Decode some data as utf-8, or if that fails, CP 1252.
pub fn decode_utf8_or_cp1252(data: Vec<u8>) -> anyhow::Result<String> {
    String::from_utf8(data).or_else(|e| {
        let mut data = e.as_bytes();
        // Prepare an output buffer with a little extra capacity to leave room for code points that
        // expand to multiple bytes in utf-8.
        let mut out = String::with_capacity(data.len() + 0x100);
        let mut decoder = encoding_rs::WINDOWS_1252.new_decoder_with_bom_removal();
        loop {
            let (res, read) = decoder.decode_to_string_without_replacement(data, &mut out, true);
            data = &data[read..];
            match res {
                DecoderResult::InputEmpty => break,
                DecoderResult::OutputFull => {
                    if data.len() > 1024 {
                        warn!("CP 1252 ran out of buffer with more than 1 KiB left!");
                        out.reserve(data.len());
                    } else {
                        // We need a little more space to finish decoding. We reserve space for
                        // every remaining input character to expand to two utf-8 bytes.
                        out.reserve(data.len() * 2);
                    }
                }
                // I'm not sure this is even possible, because I think probably every byte is valid
                // in this codepage.
                DecoderResult::Malformed(_, _) => bail!("Data is malformed for CP 1215"),
            }
        }
        Ok(out)
    })
}

/// Helper to deserialize a Vec<MegPathBuf>.
pub fn deserialize_asset_paths<'de, D>(deserializer: D) -> Result<Vec<MegPathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    let data = Vec::<TrimToMegPathBuf>::deserialize(deserializer)?;
    let (ptr, len, cap) = data.into_raw_parts();
    // TrimToMegPathBuf has the same layout as MegPathBuf by repr(transparent).
    let ptr = ptr.cast::<MegPathBuf>();
    // SAFETY: The len and cap are fine because we just got them out of a Vec. The ptr should be
    // fine because it was cast from another pointer with the exact same layout.
    let data = unsafe { Vec::from_raw_parts(ptr, len, cap) };
    Ok(data)
}

/// Helper to deserialize a [MegPathBuf].
pub fn deserialize_asset_path<'de, D>(deserializer: D) -> Result<MegPathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    TrimToMegPathBuf::deserialize(deserializer).map(Into::into)
}

/// Helper to deserialize an [`Option<MegPathBuf>`].
pub fn deserialize_opt_asset_path<'de, D>(deserializer: D) -> Result<Option<MegPathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<TrimToMegPathBuf>::deserialize(deserializer).map(|path| path.map(Into::into))
}

/// Deserialization utility wrapper for a [MegPathBuf] which trims a value before trying to parse it
/// as a MegPathBuf.
#[repr(transparent)]
struct TrimToMegPathBuf(MegPathBuf);

impl From<TrimToMegPathBuf> for MegPathBuf {
    fn from(value: TrimToMegPathBuf) -> Self {
        value.0
    }
}

impl<'de> Deserialize<'de> for TrimToMegPathBuf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TrimToMegPathBufVisitor;

        impl<'de> Visitor<'de> for TrimToMegPathBufVisitor {
            type Value = TrimToMegPathBuf;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a relative windows-style path")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                match MegPath::from_str(v.trim()) {
                    Ok(mp) => Ok(TrimToMegPathBuf(mp.to_owned())),
                    Err(e) => Err(E::custom(e)),
                }
            }
        }

        deserializer.deserialize_string(TrimToMegPathBufVisitor)
    }
}
