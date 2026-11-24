use proc_macro2::TokenStream;
use syn::{Attribute, Data, DeriveInput, Error, Expr, Result, Type};

/// Derives the `FromByteBlock` trait for a struct.
pub(crate) fn derive_from_byte_block(input: DeriveInput) -> Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "FromByteBlock can only be derived for structs",
        ));
    };

    let container_attrs = ByteBlockStructAttrs::parse(&input.attrs)?;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();


    todo!()
}

/// Derive attributes that apply to the structure that ByteBlock is being derived on.
#[derive(Default)]
struct ByteBlockStructAttrs {
    /// Number of bytes of padding to apply at the end of the struct.
    pad_after: Option<Expr>,
}

impl ByteBlockStructAttrs {
    /// Find the `byte_block` struct attributes in the given attr list.
    fn parse(attrs: &[Attribute]) -> Result<Self> {
        let mut out = Self::default();
        for attr in attrs {
            if !attr.path().is_ident("byte_block") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("pad_after") {
                    if out.pad_after.is_some() {
                        return Err(meta.error("`pad_after` was specified more than once"));
                    }
                    let value = meta.value()?;
                    out.pad_after = Some(value.parse()?);
                    Ok(())
                } else {
                    Err(meta.error(
                        "unsupported #[byte_block] container attribute; expected `pad_after = ...`",
                    ))
                }
            })?;
        }
        Ok(out)
    }
}

/// Derive attributes that apply to the fields of a ByteBlock struct.
#[derive(Default)]
struct ByteBlockFieldAttrs {
    /// Number of bytes of padding to apply at the end of the struct.
    wire_format: Option<Type>,
}

impl ByteBlockFieldAttrs {
    /// Find the `byte_block` struct attributes in the given attr list.
    fn parse(attrs: &[Attribute]) -> Result<Self> {
        let mut out = Self::default();
        for attr in attrs {
            if !attr.path().is_ident("byte_block") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("wire_format") {
                    if out.wire_format.is_some() {
                        return Err(meta.error("`wire_format` was specified more than once"));
                    }
                    let value = meta.value()?;
                    out.wire_format = Some(value.parse()?);
                    Ok(())
                } else {
                    Err(meta.error(
                        "unsupported #[byte_block] field attribute; expected `wire_format = ...`",
                    ))
                }
            })?;
        }
        Ok(out)
    }
}
