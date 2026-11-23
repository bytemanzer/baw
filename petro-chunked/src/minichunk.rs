use std::marker::PhantomData;

use crate::ChunkError;
use crate::byteblock::{ConvertBytes, FromByteBlock, TakeBytes};
use crate::error::ChunkErrorContext;

/// Interprets the given bytes as containing mini-chunks and gets an iterator over them.
pub fn iter_mini_chunks<'b>(data: &'b [u8]) -> MiniChunksIter<'b> {
    MiniChunksIter { data }
}

/// Iterator over [`MiniChunk`]s in some source data.
pub struct MiniChunksIter<'b> {
    /// Source bytes to extract chunks from.
    data: &'b [u8],
}

impl<'b> MiniChunksIter<'b> {
    /// Peek the ID of the next mini-chunk, if there are any left.
    pub fn peek_id(&self) -> Result<Option<u8>, ChunkError> {
        let mut data = self.data;
        data.take()
    }

    /// Get the next chunk if it matches the given matcher otherwise error.
    pub fn expect_chunk<M: MiniChunkIdMatcher>(
        &mut self,
        matcher: M,
    ) -> Result<MiniChunk<'b>, ChunkError> {
        match self.next() {
            None => Err(ChunkError::EndOfChunks),
            Some(Err(e)) => Err(e),
            Some(Ok(chunk)) if matcher.matches(chunk.id()) => Ok(chunk),
            Some(Ok(chunk)) => Err(ChunkError::UnexpectedChunk {
                id: chunk.id() as u32,
            }),
        }
    }

    /// Get the next chunk if it matches the given matcher, otherwise return None.
    pub fn try_expect_chunk<M: MiniChunkIdMatcher>(
        &mut self,
        matcher: M,
    ) -> Result<Option<MiniChunk<'b>>, ChunkError> {
        if let Some(id) = self.peek_id()?
            && matcher.matches(id)
        {
            return self.expect_chunk(id).map(Some);
        }
        Ok(None)
    }

    /// Produce an error if there are chunks/data left over.
    pub fn expect_empty(mut self) -> Result<(), ChunkError> {
        match self.next() {
            Some(Err(e)) => Err(e),
            Some(Ok(chunk)) => Err(ChunkError::UnexpectedChunk {
                id: chunk.id() as u32,
            }),
            None => Ok(()),
        }
    }

    /// Applies a sequence expectation to this mini chunk iterator. Must match the entire mini-chunk
    /// iterator!
    #[deprecated(note = "use `#[derive(MiniChunkStructure)]` with `MiniChunksIter::expect_struct` instead")]
    #[allow(deprecated)]
    pub fn expect_sequence<S: SequenceExpectation>(
        mut self,
        expected: S,
    ) -> Result<S::Output<'b>, ChunkError> {
        let res = expected.collect(&mut self)?;
        self.expect_empty()?;
        Ok(res)
    }

    /// Applies a map collector to this mini chunk iterator.
    pub fn expect_struct<M: MiniChunkStructure<'b>>(self) -> Result<M, ChunkError> {
        M::collect(self)
    }
}

impl<'b> Iterator for MiniChunksIter<'b> {
    type Item = Result<MiniChunk<'b>, ChunkError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            None
        } else {
            Some(self.data.take())
        }
    }
}

/// Utility to match mini-chunk IDs.
pub trait MiniChunkIdMatcher {
    /// Return true if the chunk ID matches this matcher.
    fn matches(&self, id: u8) -> bool;
}

impl MiniChunkIdMatcher for u8 {
    fn matches(&self, id: u8) -> bool {
        *self == id
    }
}

impl<const SIZE: usize> MiniChunkIdMatcher for [u8; SIZE] {
    fn matches(&self, id: u8) -> bool {
        self.contains(&id)
    }
}

impl MiniChunkIdMatcher for &[u8] {
    fn matches(&self, id: u8) -> bool {
        self.contains(&id)
    }
}

impl<F: Fn(u8) -> bool> MiniChunkIdMatcher for F {
    fn matches(&self, id: u8) -> bool {
        self(id)
    }
}

/// Represents a single mini-chunk from a Petroglyph chunked file.
pub struct MiniChunk<'b> {
    /// ID of this chunk.
    id: u8,
    /// Unparsed content of this chunk.
    content: &'b [u8],
}

impl<'b> MiniChunk<'b> {
    /// Gets the ID of the mini-chunk.
    pub fn id(&self) -> u8 {
        self.id
    }

    /// Gets the contents of the Mini Chunk.
    pub fn content(&self) -> &'b [u8] {
        self.content
    }
}

impl<'b> ConvertBytes<'b> for MiniChunk<'b> {
    fn convert<T: FromByteBlock<'b>>(&self) -> Result<T, ChunkError> {
        self.content.convert().err_to_mini_chunk_data(self.id)
    }
}

impl<'b> FromByteBlock<'b> for MiniChunk<'b> {
    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), ChunkError> {
        let (id, len): (u8, u8) = bytes.take().err_to_mini_header()?;
        let content = bytes.take_bytes(len as usize).err_to_mini_chunk_data(id)?;
        let chunk = MiniChunk { id, content };
        Ok((chunk, bytes))
    }
}

#[deprecated(note = "use `#[derive(MiniChunkStructure)]` instead")]
pub trait SequenceExpectation {
    type Output<'a>;

    fn collect<'a>(
        &self,
        mini_chunks: &mut MiniChunksIter<'a>,
    ) -> Result<Self::Output<'a>, ChunkError>;
}

/// Required mini-chunk.
#[deprecated(note = "use `#[derive(MiniChunkStructure)]` instead")]
#[derive(Copy, Clone)]
pub struct Require<T> {
    id: u8,
    _output: PhantomData<T>,
}

#[allow(deprecated)]
impl<T> SequenceExpectation for Require<T>
where
    T: for<'a> FromByteBlock<'a>,
{
    type Output<'a> = T;

    fn collect<'a>(
        &self,
        mini_chunks: &mut MiniChunksIter<'a>,
    ) -> Result<Self::Output<'a>, ChunkError> {
        let data = mini_chunks.expect_chunk(self.id)?;
        data.convert()
    }
}

/// Creates an expectation for a required element in a mini chunk sequence.
#[deprecated(note = "use `#[derive(MiniChunkStructure)]` instead")]
#[allow(deprecated)]
pub fn require<T>(id: u8) -> Require<T> {
    Require {
        id,
        _output: PhantomData,
    }
}

/// Optional mini-chunk.
#[deprecated(note = "use `#[derive(MiniChunkStructure)]` instead")]
#[derive(Copy, Clone)]
pub struct Opt<T> {
    id: u8,
    _output: PhantomData<T>,
}

/// Creates an expectation for an optional element in a mini chunk sequence.
#[deprecated(note = "use `#[derive(MiniChunkStructure)]` instead")]
#[allow(deprecated)]
pub fn opt<T>(id: u8) -> Opt<T> {
    Opt {
        id,
        _output: PhantomData,
    }
}

#[allow(deprecated)]
impl<T> SequenceExpectation for Opt<T>
where
    T: for<'a> FromByteBlock<'a>,
{
    type Output<'a> = Option<T>;

    fn collect<'a>(
        &self,
        mini_chunks: &mut MiniChunksIter<'a>,
    ) -> Result<Self::Output<'a>, ChunkError> {
        match mini_chunks.try_expect_chunk(self.id)? {
            None => Ok(None),
            Some(data) => data.convert::<T>().map(Some),
        }
    }
}

macro_rules! tuple_expectation {
    () => {
      tuple_expectation!(@inner [ ]);
    };
    ($t:ident $($rest:ident)*) => {
      tuple_expectation!($($rest)*);
      tuple_expectation!(@inner [ $t $($rest)* ]);
    };
    (@inner [ $($t:ident)* ]) => {
        #[allow(deprecated)]
        impl<$($t,)*> SequenceExpectation for ( $($t,)* )
        where
        $(
            $t: SequenceExpectation,
        )*
        {
            type Output<'a> = (
                $( <$t as SequenceExpectation>::Output<'a>, )*
            );

            #[allow(unused)]
            fn collect<'a>(&self, mini_chunks: &mut MiniChunksIter<'a>) -> Result<Self::Output<'a>, ChunkError> {
                #[allow(non_snake_case)]
                let (
                    $( $t, )*
                ) = self;

                Ok((
                    $( $t.collect(mini_chunks)?, )*
                ))
            }
        }
    };
}

tuple_expectation!(A B C D E F G H I J K L M N O P Q R S T U V W X Y Z);

/// A structure which is produced from mini-chunks.
pub trait MiniChunkStructure<'a> {
    fn collect(mini_chunks: MiniChunksIter<'a>) -> Result<Self, ChunkError>
    where
        Self: Sized;
}

#[cfg(feature = "derive")]
pub use petro_chunked_derive::MiniChunkStructure;

#[doc(hidden)]
#[deprecated(note = "use `#[derive(MiniChunkStructure)]` instead")]
#[macro_export]
macro_rules! __mini_chunk_map {
    ($result:ident <$lt:lifetime> {
        $( [$($id:tt)+] $var:ident: $t:ty $(as $mod:ident)?, )*
    }) => {
        struct $result <$lt> {
            $( $var: $crate::__mini_chunk_map!(@make_out_type $t $(, $mod)?), )*
        }

        $crate::__mini_chunk_map!(@impl<$lt> $result<$lt> {
            $( [$($id)+] $var: $t $(as $mod)?, )*
        });
    };
    ($result:ident {
        $( [$($id:tt)+] $var:ident: $t:ty $(as $mod:ident)?, )*
    }) => {
        struct $result {
            $( $var: $crate::__mini_chunk_map!(@make_out_type $t $(, $mod)?), )*
        }

        $crate::__mini_chunk_map!(@impl<'a> $result {
            $( [$($id)+] $var: $t $(as $mod)?, )*
        });
    };
    (@impl<$lt:lifetime> $result:ty {
        $( [$($id:tt)+] $var:ident: $t:ty $(as $mod:ident)?, )*
    }) => {
        impl<$lt> $crate::minichunk::MiniChunkStructure<$lt> for $result {
            fn collect(mini_chunks: $crate::minichunk::MiniChunksIter<'a>) -> Result<Self, $crate::ChunkError> {
                $( let mut $var = $crate::__mini_chunk_map!(@make_collector $t $(, $mod )?); )*

                for chunk in mini_chunks {
                    let chunk = chunk?;
                    match chunk.id() {
                        $(
                            $($id)+ => $var.add_value(
                                $($id)+,
                                stringify!($var),
                                <$crate::minichunk::MiniChunk<$lt> as $crate::byteblock::ConvertBytes<$lt>>::convert::<$t>(&chunk)?
                            )?,
                        )*
                        id => return Err($crate::_macro_util::unexpected_mini_chunk(id)),
                    }
                }

                Ok(Self {
                    $( $var: $var.finish($($id)+, stringify!($var))? ),*
                })
            }
        }
    };
    (@make_collector $t:ty) => {
        $crate::__mini_chunk_map!(@make_collector $t, required)
    };
    (@make_collector $t:ty, required) => {
        $crate::_macro_util::RequiredMapEntry::<$t>::new()
    };
    (@make_collector $t:ty, optional) => {
        $crate::_macro_util::OptionalMapEntry::<$t>::new()
    };
    (@make_collector $t:ty, repeated) => {
        $crate::_macro_util::RepeatedMapEntry::<$t>::new()
    };
    (@make_out_type $t:ty) => {
        $crate::__mini_chunk_map!(@make_out_type $t, required)
    };
    (@make_out_type $t:ty, required) => {
        $t
    };
    (@make_out_type $t:ty, optional) => {
        std::option::Option<$t>
    };
    (@make_out_type $t:ty, repeated) => {
        std::vec::Vec<$t>
    };
}

/// Create a map-based mini-chunk expectation.
#[deprecated(note = "use `#[derive(MiniChunkStructure)]` instead")]
#[allow(deprecated)]
pub use __mini_chunk_map as mini_chunk_map;

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::{MiniChunksIter, mini_chunk_map};

    mod ids {
        pub const ID2: u8 = 2;
    }

    #[test]
    fn mini_chunk_maps() {
        mini_chunk_map! {
            Mapped<'a> {
                [0x01] c01: &'a str,
                [ids::ID2] c02: &'a str as repeated,
                [0x03] c03: &'a str as optional,
                [0x04] c04: &'a str as required,
                [0x05] c05: &'a str as optional,
                [0x06] c06: &'a str,
            }
        }

        let iter = MiniChunksIter {
            data: b"\x01\x03ab\x00\
                    \x02\x02c\x00\
                    \x04\x04def\x00\
                    \x02\x05mnop\x00\
                    \x03\x07ghijkl\x00\
                    \x06\x03qr\x00",
        };
        let Mapped {
            c01,
            c02,
            c03,
            c04,
            c05,
            c06,
        } = iter.expect_struct().unwrap();

        assert_eq!(c01, "ab");
        assert_eq!(c02, ["c", "mnop"]);
        assert_eq!(c03, Some("ghijkl"));
        assert_eq!(c04, "def");
        assert_eq!(c05, None);
        assert_eq!(c06, "qr");
    }
}
