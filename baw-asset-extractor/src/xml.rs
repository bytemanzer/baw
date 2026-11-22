//! Provides helpers for dealing with encoding jank in xml files.

use std::fmt;
use std::io::Read;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use anyhow::bail;
use encoding_rs::DecoderResult;
use petro_meg::path::{MegPath, MegPathBuf};
use serde::de::{self, DeserializeOwned, Error as DeError, VariantAccess, Visitor};
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

/// Deserialization utility wrapper for a [MegPathBuf] which trims a value before trying to parse it
/// as a MegPathBuf.
#[derive(Clone)]
#[repr(transparent)]
pub struct MegPathXml(pub MegPathBuf);

impl From<MegPathXml> for MegPathBuf {
    fn from(value: MegPathXml) -> Self {
        value.0
    }
}

impl AsRef<MegPathBuf> for MegPathXml {
    fn as_ref(&self) -> &MegPathBuf {
        &self.0
    }
}

impl AsRef<MegPath> for MegPathXml {
    fn as_ref(&self) -> &MegPath {
        &self.0
    }
}

impl Deref for MegPathXml {
    type Target = MegPathBuf;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MegPathXml {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Debug for MegPathXml {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for MegPathXml {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<'de> Deserialize<'de> for MegPathXml {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct TrimToMegPathBufVisitor;

        impl<'de> Visitor<'de> for TrimToMegPathBufVisitor {
            type Value = MegPathXml;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a relative windows-style path")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                match MegPath::from_str(v.trim()) {
                    Ok(mp) => Ok(MegPathXml(mp.to_owned())),
                    Err(e) => Err(E::custom(e)),
                }
            }
        }

        deserializer.deserialize_string(TrimToMegPathBufVisitor)
    }
}

/// A helper to deserialize XML while capturing the name of the tag that the value was serialized
/// with.
///
/// The idea is that this will match any tag name and attempt to deserialize it as `T`, but that it
/// will remember what the arbitrary tag name *was* so you can inspect it later.
#[derive(Debug)]
pub struct TaggedXml<T> {
    /// The name of the xml tag that was used for the value.
    pub tag: String,
    /// The value deserialized from within that tag.
    pub value: T,
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for TaggedXml<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for Visitor<T> {
            type Value = TaggedXml<T>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("any XML element")
            }

            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: de::EnumAccess<'de>,
            {
                let (tag, variant) = data.variant::<String>()?;
                let value = variant.newtype_variant::<T>()?;
                Ok(TaggedXml { tag, value })
            }
        }

        // We accomplish our tag capture by pretending to be an enum with unknown variants. Serde
        // should then give us the outer tag as the enum variant.
        deserializer.deserialize_enum("", &[], Visitor(PhantomData))
    }
}

/// Serde helper which pretends to be a sequence but only captures the last value.
///
/// This wouldn't work for most serializers, but should work for quick-xml since
/// quick-xml supports disjoint repeated fields.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Last<T>(pub T);

impl From<Last<XmlF32>> for f32 {
    fn from(value: Last<XmlF32>) -> Self {
        value.0.0
    }
}

impl<T> AsRef<T> for Last<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for Last<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Last<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: fmt::Debug> fmt::Debug for Last<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Last<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for Visitor<T> {
            type Value = Last<T>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a {}", std::any::type_name::<T>())
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut last = None;
                while let Some(next) = seq.next_element()? {
                    last = Some(next);
                }
                match last {
                    None => Err(<A::Error as de::Error>::invalid_length(0, &self)),
                    Some(last) => Ok(Last(last)),
                }
            }
        }

        deserializer.deserialize_seq(Visitor(PhantomData))
    }
}

/// Serde helper which pretends to be a sequence but only captures the last value, if any.
///
/// This wouldn't work for most serializers, but should work for quick-xml since
/// quick-xml supports disjoint repeated fields.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct OptLast<T>(pub Option<T>);

impl<T> Default for OptLast<T> {
    fn default() -> Self {
        OptLast(None)
    }
}

impl<T> OptLast<T> {
    /// Convenience helper to convert the inner value or fall back to a default.
    pub fn unwrap_or<U>(self, default: U) -> U
    where
        T: Into<U>,
    {
        match self.0 {
            Some(val) => val.into(),
            None => default,
        }
    }
}

impl<T: AsRef<str>> OptLast<T> {
    /// If `T` is `AsRef<str>`, maps self to an optional `&str`.
    pub fn as_str(&self) -> Option<&str> {
        match self.0 {
            Some(ref v) => Some(v.as_ref()),
            None => None,
        }
    }
}

impl From<OptLast<XmlF32>> for Option<f32> {
    fn from(value: OptLast<XmlF32>) -> Self {
        match value.0 {
            Some(v) => Some(v.into()),
            None => None,
        }
    }
}

impl<T> AsRef<Option<T>> for OptLast<T> {
    fn as_ref(&self) -> &Option<T> {
        &self.0
    }
}

impl<T> Deref for OptLast<T> {
    type Target = Option<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for OptLast<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: fmt::Debug> fmt::Debug for OptLast<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for OptLast<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for Visitor<T> {
            type Value = OptLast<T>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "a optional {}", std::any::type_name::<T>())
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut last = None;
                while let Some(next) = seq.next_element()? {
                    last = Some(next);
                }
                Ok(OptLast(last))
            }
        }

        deserializer.deserialize_seq(Visitor(PhantomData))
    }
}

/// Wrapper for f32 that support c-style 'f' suffixes.
#[derive(Copy, Clone)]
pub struct XmlF32(pub f32);

impl From<XmlF32> for f32 {
    fn from(value: XmlF32) -> Self {
        value.0
    }
}

impl AsRef<f32> for XmlF32 {
    fn as_ref(&self) -> &f32 {
        &self.0
    }
}

impl Deref for XmlF32 {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for XmlF32 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Debug for XmlF32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for XmlF32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<'de> Deserialize<'de> for XmlF32 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = XmlF32;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a float value")
            }

            fn visit_f64<E: DeError>(self, v: f64) -> Result<Self::Value, E> {
                warn!("loss of precision, downcasting deserialized f64 to f32");
                self.visit_f32(v as f32)
            }

            fn visit_f32<E: DeError>(self, v: f32) -> Result<Self::Value, E> {
                Ok(XmlF32(v))
            }

            fn visit_str<E: DeError>(self, s: &str) -> Result<Self::Value, E> {
                let v = s.trim();
                let v = v.strip_suffix(['f', 'F']).unwrap_or(v);
                match v.parse() {
                    Ok(v) => Ok(XmlF32(v)),
                    Err(_) => Err(E::invalid_value(de::Unexpected::Str(s), &self)),
                }
            }
        }

        deserializer.deserialize_f32(Visitor)
    }
}

/// Wrapper for String which trims and folds to ascii lowercase, useful for "identifiers" in the
/// XML, which are often case-insensitive and should not contain whitespace.
///
/// We wrap String with case-folding rather than AinString because we in Bevy, we want to be able to
/// use normal string/str (so we'll need to coerce to String at some point anyway), and because this
/// gives us a little more flexibility just in case some mod has an ill-advised unicode character in
/// a unit name.
#[derive(Clone)]
pub struct XmlIdent(pub String);

impl XmlIdent {
    /// Gets an `&str` reference to the value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<XmlIdent> for String {
    fn from(value: XmlIdent) -> Self {
        value.0
    }
}

impl AsRef<String> for XmlIdent {
    fn as_ref(&self) -> &String {
        &self.0
    }
}

impl AsRef<str> for XmlIdent {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for XmlIdent {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for XmlIdent {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Debug for XmlIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for XmlIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl<'de> Deserialize<'de> for XmlIdent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = XmlIdent;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string value")
            }

            fn visit_str<E: DeError>(self, s: &str) -> Result<Self::Value, E> {
                Ok(XmlIdent(s.trim().to_ascii_lowercase()))
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}
