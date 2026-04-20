//! Provides utilites for taking bytes out of a chunked file.
use std::marker::PhantomData;

use byteorder::{ByteOrder, LE};
use thiserror::Error;

/// Error returned when a Read call wouldn't have enough data to get the requested amount of
/// content.
#[derive(Error, Debug)]
#[error(
    "Not enough data to take the requested number of bytes. Requested: {requested}, Available: \
    {available}"
)]
pub struct NotEnoughData {
    /// Number of bytes requested to read.
    pub requested: usize,
    /// Number of bytes that were available to read.
    pub available: usize,
}

/// Trait for taking bytes from a byte slice. Unlike [std::io::Read], it directly returns bytes and
/// the only error is 'insufficient data'.
pub trait TakeBytes<'b> {
    /// Take the given number of bytes from this byte slice. If successful the slice will be
    /// resliced to remove the taken data. Equivalent to:
    ///
    /// ```
    /// # use petro_chunked::byteblock::NotEnoughData;
    /// fn take_bytes<'b>(data: &mut &'b[u8], n: usize) -> Result<&'b [u8], NotEnoughData> {
    ///     if data.len() < n {
    ///         return Err(NotEnoughData {
    ///             requested: n,
    ///             available: data.len(),
    ///         });
    ///     }
    ///     let result;
    ///     (result, *data) = data.split_at(n);
    ///     Ok(result)
    /// }
    /// # let mut data: &[u8] = b"Hello World";
    /// # assert!(matches!(take_bytes(&mut data, 5), Ok(b"Hello")));
    /// # assert!(matches!(take_bytes(&mut data, 7), Err(NotEnoughData { requested: 7, available: 6 })));
    /// ```
    fn take_bytes(&mut self, n: usize) -> Result<&'b [u8], NotEnoughData>;

    /// Take a value off the data. Value must implement FixedSizeTakeable.
    fn take<T: FromByteBlock<'b>>(&mut self) -> Result<T, T::Error>;

    /// Utility to map the result of taking some data, where the type to take can be inferred from
    /// the function's argument type.
    ///
    /// ```
    /// # use petro_chunked::byteblock::TakeBytes;
    /// #[derive(Eq, PartialEq, Debug)]
    /// struct Example {
    ///     a: u16,
    ///     b: u8,
    ///     c: u8,
    /// }
    /// let mut data: &[u8] = &[0x12, 0x34, 0x56, 0x78];
    /// let ex = data.take_map(|(a, b, c)| Example { a, b, c }).unwrap();
    /// assert_eq!(ex, Example { a: 0x3412, b: 0x56, c: 0x78 });
    /// ```
    fn take_map<F, T, U>(&mut self, func: F) -> Result<U, T::Error>
    where
        F: FnOnce(T) -> U,
        T: FromByteBlock<'b>,
    {
        self.take::<T>().map(func)
    }

    /// Returns an iterator which will repeatedly try to take elements out of this source until the
    /// source is empty.
    ///
    /// Note that discarding the iterator will stop taking elements at that point, meaning you can
    /// safely do something like take_iter().take(4) and then resume using this source after the
    /// iterator limit is hit.
    fn take_iter<'a, T: FromByteBlock<'b>>(&'a mut self) -> TakeIter<'a, 'b, T>;
}

impl<'b> TakeBytes<'b> for &'b [u8] {
    fn take_bytes(&mut self, n: usize) -> Result<&'b [u8], NotEnoughData> {
        if self.len() < n {
            return Err(NotEnoughData {
                requested: n,
                available: self.len(),
            });
        }
        let result;
        (result, *self) = self.split_at(n);
        Ok(result)
    }

    fn take<T: FromByteBlock<'b>>(&mut self) -> Result<T, T::Error> {
        let (res, rest) = T::take_from(*self)?;
        *self = rest;
        Ok(res)
    }

    fn take_iter<'a, T: FromByteBlock<'b>>(&'a mut self) -> TakeIter<'a, 'b, T> {
        TakeIter {
            data: self,
            _ty: PhantomData,
        }
    }
}

/// An iterator which repeatedly takes elements of some type from the underlying data.
pub struct TakeIter<'a, 'b, T> {
    /// Remaining data to take from.
    data: &'a mut &'b [u8],
    _ty: PhantomData<T>,
}

impl<'a, 'b, T: FromByteBlock<'b>> Iterator for TakeIter<'a, 'b, T> {
    type Item = Result<T, T::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.data.is_empty() {
            None
        } else {
            Some(self.data.take())
        }
    }
}

/// Helper for taking some data from a block of bytes.
pub trait FromByteBlock<'b> {
    type Error;

    /// Take this item from the given byte block, returning self and the remaining unused bytes.
    ///
    /// Returning the "rest" allow this to be used in combinators in a way that ensures that no
    /// modifications are made to the root unless all children succeed.
    fn take_from(bytes: &'b [u8]) -> Result<(Self, &'b [u8]), Self::Error>
    where
        Self: Sized;
}

/// Helper to make any fixed size type wowrk with TakeBytes.
///
/// Note that the conversion from bytes must be infallible.
pub trait FromFixedByteBlock<'b> {
    /// Size of data this type will take from TakeBytes.
    const SIZE: usize;

    /// Construct this takeable from a slice of the correct number of bytes.
    ///
    /// Caller is responsible for ensuring that the size of `bytes` is exactly equal to `SIZE`, so
    /// this method should just panic if the number of bytees is wrong.
    fn from_bytes(bytes: &'b [u8]) -> Self;

    /// Try to construct this block type from the given bytes, if there is enough bytes.
    fn try_from_bytes(bytes: &'b [u8]) -> Result<Self, NotEnoughData>
    where
        Self: Sized,
    {
        let (res, _) = Self::take_from(bytes)?;
        Ok(res)
    }
}

impl<'b, T> FromByteBlock<'b> for T
where
    T: FromFixedByteBlock<'b>,
{
    type Error = NotEnoughData;

    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), NotEnoughData>
    where
        Self: Sized,
    {
        let res = Self::from_bytes(bytes.take_bytes(Self::SIZE)?);
        Ok((res, bytes))
    }
}

impl<'b> FromFixedByteBlock<'b> for u8 {
    const SIZE: usize = size_of::<u8>();

    fn from_bytes(bytes: &'b [u8]) -> Self {
        bytes[0]
    }
}

impl<'b> FromFixedByteBlock<'b> for i8 {
    const SIZE: usize = size_of::<i8>();

    fn from_bytes(bytes: &'b [u8]) -> Self {
        bytes[0] as i8
    }
}

macro_rules! le_takeable {
    ($($t:ty => $method:ident,)*) => { $(
        impl<'b> FromFixedByteBlock<'b> for $t {
            const SIZE: usize = size_of::<$t>();

            fn from_bytes(bytes: &'b [u8]) -> Self {
                LE::$method(bytes)
            }
        }
    )* };
}

le_takeable!(
    u16 => read_u16,
    i16 => read_i16,
    u32 => read_u32,
    i32 => read_i32,
    f32 => read_f32,
    u64 => read_u64,
    i64 => read_i64,
    f64 => read_f64,
);

impl<'b, const N: usize> FromFixedByteBlock<'b> for &'b [u8; N] {
    const SIZE: usize = N;

    fn from_bytes(bytes: &'b [u8]) -> Self {
        bytes.as_array().unwrap()
    }
}

impl<'b, T, const N: usize> FromFixedByteBlock<'b> for [T; N]
where
    T: FromFixedByteBlock<'b>,
{
    const SIZE: usize = N * T::SIZE;

    fn from_bytes(bytes: &'b [u8]) -> Self {
        std::array::from_fn(|idx| {
            let offset = idx * T::SIZE;
            T::from_bytes(&bytes[offset..offset + T::SIZE])
        })
    }
}

impl<'b, T: FromByteBlock<'b>> FromByteBlock<'b> for Vec<T> {
    type Error = T::Error;

    fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), T::Error>
    where
        Self: Sized,
    {
        let res = bytes.take_iter::<T>().collect::<Result<_, _>>()?;
        Ok((res, bytes))
    }
}

macro_rules! tuple_takeable {
    () => {
        impl<'b> FromFixedByteBlock<'b> for () {
            const SIZE: usize = 0;

            fn from_bytes(_: &'b [u8]) -> Self { }
        }
    };
    ($t:ident $($rest:ident)*) => {
      tuple_takeable!($($rest)*);
      tuple_takeable!(@inner [ $t $($rest)* ]);
    };
    (@inner [ $t:ident $($rest:ident)* ]) => {
        impl<'b, $t, $($rest,)*> FromByteBlock<'b> for ( $t, $($rest,)* )
        where
            $t: FromByteBlock<'b>,
        $(
            $rest: FromByteBlock<'b>,
            $t::Error: From<$rest::Error>,
        )*
        {
            type Error = $t::Error;

            #[allow(unused)]
            fn take_from(mut bytes: &'b [u8]) -> Result<(Self, &'b [u8]), Self::Error> {
                #[allow(non_snake_case)]
                let $t = bytes.take::<$t>()?;
                $(
                    #[allow(non_snake_case)]
                    let $rest = bytes.take::<$rest>()?;
                )*

                Ok((( $t, $( $rest, )* ), bytes))
            }
        }
    };
}

tuple_takeable!(A B C D E F G H I J K L M N O P Q R S T U V W X Y Z);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_bytes_test() {
        let mut data: &[u8] = b"Hello World";
        assert!(matches!(data.take_bytes(5), Ok(b"Hello")));
        assert_eq!(data, b" World");
        assert!(matches!(
            data.take_bytes(7),
            Err(NotEnoughData {
                requested: 7,
                available: 6
            })
        ));
        assert_eq!(data, b" World");
    }

    #[test]
    fn int_take_test() {
        let mut data: &[u8] = &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        assert_eq!(data.take::<u8>().unwrap(), 0x12);
        assert_eq!(data.take::<i8>().unwrap(), 0x34);
        assert_eq!(data.take::<u16>().unwrap(), 0x7856);
        assert_eq!(data.take::<i16>().unwrap(), -0x4366);
        assert_eq!(data, b"");

        let mut data: &[u8] = &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
        assert_eq!(data.take::<u32>().unwrap(), 0x78563412);
        assert_eq!(data.take::<i32>().unwrap(), -0x0f214366);
        assert_eq!(data, b"");

        let mut data: &[u8] = &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
        assert_eq!(data.take::<u64>().unwrap(), 0xf0debc9a78563412);
        assert_eq!(data, b"");

        let mut data: &[u8] = &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
        assert_eq!(data.take::<i64>().unwrap(), -0x0f21436587a9cbee);
        assert_eq!(data, b"");
    }

    #[test]
    fn tuple_take_test() {
        let mut data: &[u8] = &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        let res: (u8, u16, i16, i8) = data.take().unwrap();
        assert_eq!(res, (0x12, 0x5634, -0x6588, -0x44));
    }

    #[test]
    fn array_take_test() {
        let mut data: &[u8] = &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
        let res: [u32; 2] = data.take().unwrap();
        assert_eq!(res, [0x78563412, 0xf0debc9a]);
    }

    #[test]
    fn vec_take_test() {
        let mut data: &[u8] = &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0];
        let res: Vec<u16> = data.take().unwrap();
        assert_eq!(res, [0x3412, 0x7856, 0xbc9a, 0xf0de]);
    }
}
