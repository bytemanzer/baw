use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{
    Attribute, Data, DeriveInput, Error, Expr, Fields, Generics, Ident, Lifetime, Result, Token,
    Type,
};

mod byteblock;
mod util;

/// Derives `petro_chunked::byteblock::FromByteBlock` for a struct.
///
/// Fields are read sequentially using `TakeBytes::take`.
///
/// ### Container / Field Attributes (`#[byte_block(...)]`)
/// - `pad_after = <expr>`: Skips `<expr>` trailing padding bytes after reading the field or struct.
/// - `wire_format = <Type>`: (Field only) Reads `<Type>` from the byte block and converts it into
///   the field's type via `From::from`.
#[proc_macro_derive(FromByteBlock, attributes(byte_block))]
pub fn derive_from_byte_block(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    byteblock::derive_from_byte_block(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Derives `petro_chunked::minichunk::MiniChunkStructure` for a struct.
///
/// ### Container Attributes (`#[mini_chunk(...)]`)
/// You must explicitly specify either `map` or `sequence`:
/// - `map`: Read fields as an unordered map keyed by mini-chunk ID.
/// - `sequence`: Read fields in strict declaration order.
/// - `ignore_unknown`: Ignore unrecognized mini-chunk IDs instead of returning an error.
///
/// ### Field Attributes (`#[mini_chunk(<id>, ...)]`)
/// - `<id>`: Required `u8` expression for the mini-chunk ID.
/// - `repeated`: Collects 0 or more mini-chunks with `<id>` into a `Vec<T>`, where each mini-chunk
///   body has type `T`. (Without `repeated`, a `Vec<T>` field requires a single mini-chunk whose
///   body is decoded as a packed list `Vec<T>`.)
/// - `optional`: Collects 0 or 1 mini-chunk with `<id>` into an `Option<T>`, where the mini-chunk
///   body has type `T`. (Without `optional`, an `Option<T>` field requires a single mini-chunk
///   whose body is decoded via `FromByteBlock for Option<T>`.)
/// - `default` / `default = <expr>`: Collects 0 or 1 mini-chunk with `<id>`, using
///   `Default::default()` or `<expr>` if absent.
/// - `wire_format = <Type>`: Specifies the wire type for a single mini-chunk's body. Reads `<Type>`
///   from the mini-chunk and converts it into the per-chunk body type `T` via `From::from`.
#[proc_macro_derive(MiniChunkStructure, attributes(mini_chunk))]
pub fn derive_mini_chunk_structure(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    expand_mini_chunk_structure(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Derives `petro_chunked::chunks::ChunkStructure` for a struct.
///
/// ### Container Attributes (`#[chunk(...)]` on structs)
/// You must explicitly specify either `map` or `sequence` on structs:
/// - `map`: Read fields as an unordered map keyed by chunk ID.
/// - `sequence`: Read fields in strict declaration order.
/// - `ignore_unknown`: Ignore unrecognized chunk IDs instead of returning an error.
///
/// ### Field Attributes (`#[chunk(...)]`)
/// - `<id>` or `stream`: Either a `u32` chunk ID expression, or `stream` to delegate to a type
///   implementing `ChunkStreamItem` (such as a chunk-tagged enum or a `#[derive(ChunkStreamItem)]`
///   sequence struct).
/// - Payload mode (for `<id>` fields):
///   - `data` (default): Reads leaf bytes via `chunk.read_data::<T>()` (`FromByteBlock`).
///   - `mini_chunks`: Reads leaf mini-chunks via `chunk.read_mini_chunks::<T>()` (`MiniChunkStructure`).
///   - `children`: Reads child chunks via `chunk.read_children::<T>()` (`ChunkStructure`).
///   - `ignore`: Ignores the chunk's payload.
/// - Cardinality:
///   - `repeated`: Collects 0 or more chunk occurrences into `Vec<T>`, where each chunk body has
///     type `T`. (Without `repeated`, a `Vec<T>` field requires a single chunk whose body is
///     decoded as a packed list `Vec<T>`.)
///   - `optional`: Collects 0 or 1 chunk occurrence into `Option<T>`, where the chunk body has
///     type `T`. (Without `optional`, an `Option<T>` field requires a single chunk whose body is
///     decoded via `FromByteBlock for Option<T>`.)
///   - `default` / `default = <expr>`: 0 or 1 occurrence, falling back to `Default::default()` or `<expr>`.
/// - `wire_format = <Type>`: Specifies the wire type for a single chunk's body. Reads `<Type>` from
///   the chunk and converts it into the per-chunk body type `T` via `From::from`.
#[proc_macro_derive(ChunkStructure, attributes(chunk))]
pub fn derive_chunk_structure(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    expand_chunk_structure(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Derives `petro_chunked::chunks::ChunkStreamItem` for a chunk-tagged enum or a sequential
/// multi-chunk struct (e.g., a submesh `(0x10100, 0x10000)` pair or particle track `(0x0, 0x1)`
/// pair).
#[proc_macro_derive(ChunkStreamItem, attributes(chunk))]
pub fn derive_chunk_stream_item(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    expand_chunk_stream_item(input)
        .unwrap_or_else(Error::into_compile_error)
        .into()
}

/// Returns `(impl_generics_with_lt, ty_generics, where_clause, lifetime)` for traits parameterized
/// by a borrow lifetime `'b` (like `FromByteBlock<'b>`, `MiniChunkStructure<'b>`, `ChunkStructure<'b>`).
fn split_generics_with_borrow_lt(
    generics: &Generics,
) -> (
    TokenStream2,
    TokenStream2,
    Option<&syn::WhereClause>,
    Lifetime,
) {
    let (_, ty_generics, where_clause) = generics.split_for_impl();
    let ty_generics_tokens = quote!(#ty_generics);

    if let Some(first_lt) = generics.lifetimes().next() {
        let lt = first_lt.lifetime.clone();
        let (impl_generics, _, _) = generics.split_for_impl();
        (quote!(#impl_generics), ty_generics_tokens, where_clause, lt)
    } else {
        let lt = Lifetime::new("'__b", Span::call_site());
        let mut params = Vec::new();
        params.push(quote!(#lt));
        for param in &generics.params {
            params.push(quote!(#param));
        }
        (quote!(<#(#params),*>), ty_generics_tokens, where_clause, lt)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContainerOrderMode {
    Map,
    Sequence,
}

// =================================================================================================
// FromByteBlock
// =================================================================================================

fn expand_from_byte_block(input: DeriveInput) -> Result<TokenStream2> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "FromByteBlock can only be derived for structs",
        ));
    };

    let container_attrs = parse_byte_block_attrs(&input.attrs, false)?;

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause, lt) =
        split_generics_with_borrow_lt(&input.generics);

    let mut stmts = Vec::new();

    let take_field_expr = |fty: &Type, wire_format: &Option<Type>| -> TokenStream2 {
        match wire_format {
            Some(wty) => quote! {
                <#fty as ::std::convert::From<#wty>>::from(
                    ::petro_chunked::byteblock::TakeBytes::take::<#wty>(&mut __bytes)?
                )
            },
            None => quote! {
                ::petro_chunked::byteblock::TakeBytes::take::<#fty>(&mut __bytes)?
            },
        }
    };

    let construct = match &data.fields {
        Fields::Named(fields) => {
            let mut field_inits = Vec::new();
            for field in &fields.named {
                let fname = field.ident.as_ref().unwrap();
                let fty = &field.ty;
                let fattrs = parse_byte_block_attrs(&field.attrs, true)?;
                let take_expr = take_field_expr(fty, &fattrs.wire_format);
                stmts.push(quote! {
                    let #fname: #fty = #take_expr;
                });
                if let Some(pad) = &fattrs.pad_after {
                    stmts.push(quote! {
                        let _ = ::petro_chunked::byteblock::TakeBytes::take_bytes(&mut __bytes, #pad)?;
                    });
                }
                field_inits.push(quote!(#fname));
            }
            quote!(Self { #(#field_inits),* })
        }
        Fields::Unnamed(fields) => {
            let mut field_inits = Vec::new();
            for (idx, field) in fields.unnamed.iter().enumerate() {
                let var = format_ident!("__field_{idx}");
                let fty = &field.ty;
                let fattrs = parse_byte_block_attrs(&field.attrs, true)?;
                let take_expr = take_field_expr(fty, &fattrs.wire_format);
                stmts.push(quote! {
                    let #var: #fty = #take_expr;
                });
                if let Some(pad) = &fattrs.pad_after {
                    stmts.push(quote! {
                        let _ = ::petro_chunked::byteblock::TakeBytes::take_bytes(&mut __bytes, #pad)?;
                    });
                }
                field_inits.push(quote!(#var));
            }
            quote!(Self(#(#field_inits),*))
        }
        Fields::Unit => quote!(Self),
    };

    if let Some(pad) = &container_attrs.pad_after {
        stmts.push(quote! {
            let _ = ::petro_chunked::byteblock::TakeBytes::take_bytes(&mut __bytes, #pad)?;
        });
    }

    Ok(quote! {
        impl #impl_generics ::petro_chunked::byteblock::FromByteBlock<#lt> for #name #ty_generics #where_clause {
            fn take_from(mut __bytes: &#lt [u8]) -> ::std::result::Result<(Self, &#lt [u8]), ::petro_chunked::ChunkError> {
                #(#stmts)*
                ::std::result::Result::Ok((#construct, __bytes))
            }
        }
    })
}

// =================================================================================================
// Common helpers for Option<T> / Vec<T> extraction & Cardinality
// =================================================================================================

fn extract_generic_inner<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    if type_path.qself.is_some() {
        return None;
    }
    let last = type_path.path.segments.last()?;
    if last.ident != wrapper {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    if args.args.len() != 1 {
        return None;
    }
    match args.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

enum Cardinality<'a> {
    Required,
    Optional(&'a Type),
    Default,
    DefaultExpr(Expr),
    Repeated(&'a Type),
}

// =================================================================================================
// MiniChunkStructure
// =================================================================================================

struct MiniChunkContainerAttrs {
    mode: ContainerOrderMode,
    ignore_unknown: bool,
}

fn parse_mini_chunk_container_attrs(input: &DeriveInput) -> Result<MiniChunkContainerAttrs> {
    let mut mode: Option<ContainerOrderMode> = None;
    let mut ignore_unknown = false;

    for attr in &input.attrs {
        if !attr.path().is_ident("mini_chunk") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("map") {
                if mode.is_some() {
                    return Err(meta.error("cannot specify both `map` and `sequence`"));
                }
                mode = Some(ContainerOrderMode::Map);
                Ok(())
            } else if meta.path.is_ident("sequence") {
                if mode.is_some() {
                    return Err(meta.error("cannot specify both `map` and `sequence`"));
                }
                mode = Some(ContainerOrderMode::Sequence);
                Ok(())
            } else if meta.path.is_ident("ignore_unknown") {
                ignore_unknown = true;
                Ok(())
            } else {
                Err(meta.error("unsupported container #[mini_chunk] attribute; expected `map`, `sequence`, or `ignore_unknown`"))
            }
        })?;
    }

    let Some(mode) = mode else {
        return Err(Error::new_spanned(
            &input.ident,
            "MiniChunkStructure requires an explicit `#[mini_chunk(map)]` or `#[mini_chunk(sequence)]` attribute on the struct",
        ));
    };

    Ok(MiniChunkContainerAttrs {
        mode,
        ignore_unknown,
    })
}

struct MiniChunkFieldSpec<'a> {
    ident: &'a Ident,
    ty: &'a Type,
    id: Expr,
    cardinality: Cardinality<'a>,
    wire_format: Option<Type>,
}

struct MiniChunkFieldAttrParsed {
    id: Expr,
    repeated: bool,
    optional: bool,
    default: bool,
    default_expr: Option<Expr>,
    wire_format: Option<Type>,
}

impl Parse for MiniChunkFieldAttrParsed {
    fn parse(input: ParseStream) -> Result<Self> {
        let id: Expr = input.parse()?;
        let mut repeated = false;
        let mut optional = false;
        let mut default = false;
        let mut default_expr = None;
        let mut wire_format = None;

        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            let kw: Ident = input.parse()?;
            if kw == "repeated" {
                repeated = true;
            } else if kw == "optional" {
                optional = true;
            } else if kw == "required" {
                // explicit default
            } else if kw == "default" {
                if input.peek(Token![=]) {
                    let _: Token![=] = input.parse()?;
                    default_expr = Some(input.parse()?);
                } else {
                    default = true;
                }
            } else if kw == "wire_format" {
                let _: Token![=] = input.parse()?;
                wire_format = Some(input.parse()?);
            } else {
                return Err(Error::new_spanned(
                    kw,
                    "expected `repeated`, `optional`, `required`, `default`, or `wire_format = ...`",
                ));
            }
        }

        Ok(Self {
            id,
            repeated,
            optional,
            default,
            default_expr,
            wire_format,
        })
    }
}

fn parse_mini_chunk_field<'a>(field: &'a syn::Field) -> Result<MiniChunkFieldSpec<'a>> {
    let ident = field
        .ident
        .as_ref()
        .ok_or_else(|| Error::new_spanned(&field.ty, "MiniChunkStructure requires named fields"))?;
    let mut parsed_attr: Option<MiniChunkFieldAttrParsed> = None;
    for attr in &field.attrs {
        if attr.path().is_ident("mini_chunk") {
            if parsed_attr.is_some() {
                return Err(Error::new_spanned(
                    &attr.meta,
                    "duplicate #[mini_chunk] attribute",
                ));
            }
            parsed_attr = Some(attr.parse_args()?);
        }
    }
    let Some(parsed) = parsed_attr else {
        return Err(Error::new_spanned(
            ident,
            "missing #[mini_chunk(<id>)] attribute on field",
        ));
    };

    let ty = &field.ty;
    let cardinality = if parsed.repeated {
        let inner = extract_generic_inner(ty, "Vec").ok_or_else(|| {
            Error::new_spanned(ty, "`repeated` mini_chunk field must have type `Vec<T>`")
        })?;
        Cardinality::Repeated(inner)
    } else if let Some(expr) = parsed.default_expr {
        Cardinality::DefaultExpr(expr)
    } else if parsed.default {
        Cardinality::Default
    } else if parsed.optional {
        let inner = extract_generic_inner(ty, "Option").ok_or_else(|| {
            Error::new_spanned(ty, "`optional` mini_chunk field must have type `Option<T>`")
        })?;
        Cardinality::Optional(inner)
    } else {
        Cardinality::Required
    };

    Ok(MiniChunkFieldSpec {
        ident,
        ty,
        id: parsed.id,
        cardinality,
        wire_format: parsed.wire_format,
    })
}

fn decode_mini_chunk_expr(
    target_ty: &Type,
    wire_format: &Option<Type>,
    chunk_ident: &Ident,
) -> TokenStream2 {
    match wire_format {
        Some(wty) => quote! {
            <#target_ty as ::std::convert::From<#wty>>::from(
                ::petro_chunked::byteblock::ConvertBytes::convert::<#wty>(&#chunk_ident)?
            )
        },
        None => quote! {
            ::petro_chunked::byteblock::ConvertBytes::convert::<#target_ty>(&#chunk_ident)?
        },
    }
}

fn expand_mini_chunk_structure(input: DeriveInput) -> Result<TokenStream2> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "MiniChunkStructure can only be derived for structs",
        ));
    };
    let Fields::Named(named) = &data.fields else {
        return Err(Error::new_spanned(
            &input.ident,
            "MiniChunkStructure requires a struct with named fields",
        ));
    };

    let container_attrs = parse_mini_chunk_container_attrs(&input)?;
    let mut specs = Vec::new();
    for f in &named.named {
        specs.push(parse_mini_chunk_field(f)?);
    }

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause, lt) =
        split_generics_with_borrow_lt(&input.generics);

    if container_attrs.mode == ContainerOrderMode::Sequence {
        let mut stmts = Vec::new();
        let mut inits = Vec::new();
        let c_var = format_ident!("__c");

        for spec in &specs {
            let fident = spec.ident;
            let fty = spec.ty;
            let id = &spec.id;
            match &spec.cardinality {
                Cardinality::Required => {
                    let decoded = decode_mini_chunk_expr(fty, &spec.wire_format, &c_var);
                    stmts.push(quote! {
                        let #c_var = __mini_chunks.expect_chunk(#id)?;
                        let #fident: #fty = #decoded;
                    });
                }
                Cardinality::Optional(inner) => {
                    let decoded = decode_mini_chunk_expr(inner, &spec.wire_format, &c_var);
                    stmts.push(quote! {
                        let #fident: #fty = match __mini_chunks.try_expect_chunk(#id)? {
                            ::std::option::Option::Some(#c_var) => ::std::option::Option::Some(#decoded),
                            ::std::option::Option::None => ::std::option::Option::None,
                        };
                    });
                }
                Cardinality::Default => {
                    let decoded = decode_mini_chunk_expr(fty, &spec.wire_format, &c_var);
                    stmts.push(quote! {
                        let #fident: #fty = match __mini_chunks.try_expect_chunk(#id)? {
                            ::std::option::Option::Some(#c_var) => #decoded,
                            ::std::option::Option::None => ::std::default::Default::default(),
                        };
                    });
                }
                Cardinality::DefaultExpr(expr) => {
                    let decoded = decode_mini_chunk_expr(fty, &spec.wire_format, &c_var);
                    stmts.push(quote! {
                        let #fident: #fty = match __mini_chunks.try_expect_chunk(#id)? {
                            ::std::option::Option::Some(#c_var) => #decoded,
                            ::std::option::Option::None => #expr,
                        };
                    });
                }
                Cardinality::Repeated(inner) => {
                    let decoded = decode_mini_chunk_expr(inner, &spec.wire_format, &c_var);
                    stmts.push(quote! {
                        let mut #fident: ::std::vec::Vec<#inner> = ::std::vec::Vec::new();
                        while let ::std::option::Option::Some(#c_var) = __mini_chunks.try_expect_chunk(#id)? {
                            #fident.push(#decoded);
                        }
                    });
                }
            }
            inits.push(quote!(#fident));
        }

        let finish_check = if container_attrs.ignore_unknown {
            quote! {
                for __rem in __mini_chunks {
                    let _ = __rem?;
                }
            }
        } else {
            quote! {
                __mini_chunks.expect_empty()?;
            }
        };

        Ok(quote! {
            impl #impl_generics ::petro_chunked::minichunk::MiniChunkStructure<#lt> for #name #ty_generics #where_clause {
                fn collect(mut __mini_chunks: ::petro_chunked::minichunk::MiniChunksIter<#lt>) -> ::std::result::Result<Self, ::petro_chunked::ChunkError> {
                    #(#stmts)*
                    #finish_check
                    ::std::result::Result::Ok(Self { #(#inits),* })
                }
            }
        })
    } else {
        let mut collectors = Vec::new();
        let mut arms = Vec::new();
        let mut finishes = Vec::new();
        let chunk_var = format_ident!("__chunk");

        for spec in &specs {
            let fident = spec.ident;
            let fname_str = fident.to_string();
            let fty = spec.ty;
            let id = &spec.id;

            let (collector_init, inner_ty, finish_expr) = match &spec.cardinality {
                Cardinality::Required => (
                    quote!(::petro_chunked::_macro_util::RequiredMapEntry::<#fty>::new()),
                    fty,
                    quote!(#fident.finish(#id, #fname_str)?),
                ),
                Cardinality::Optional(inner) => (
                    quote!(::petro_chunked::_macro_util::OptionalMapEntry::<#inner>::new()),
                    *inner,
                    quote!(#fident.finish(#id, #fname_str)?),
                ),
                Cardinality::Default => (
                    quote!(::petro_chunked::_macro_util::OptionalMapEntry::<#fty>::new()),
                    fty,
                    quote!(#fident.finish_or_default()?),
                ),
                Cardinality::DefaultExpr(expr) => (
                    quote!(::petro_chunked::_macro_util::OptionalMapEntry::<#fty>::new()),
                    fty,
                    quote!(#fident.finish_or_else(|| #expr)?),
                ),
                Cardinality::Repeated(inner) => (
                    quote!(::petro_chunked::_macro_util::RepeatedMapEntry::<#inner>::new()),
                    *inner,
                    quote!(#fident.finish(#id, #fname_str)?),
                ),
            };

            let decoded = decode_mini_chunk_expr(inner_ty, &spec.wire_format, &chunk_var);

            collectors.push(quote! {
                let mut #fident = #collector_init;
            });
            arms.push(quote! {
                __id if __id == #id => {
                    #fident.add_value(__id, #fname_str, #decoded)?;
                }
            });
            finishes.push(quote! {
                #fident: #finish_expr
            });
        }

        let fallback_arm = if container_attrs.ignore_unknown {
            quote!(_ => {})
        } else {
            quote!(__id => return ::std::result::Result::Err(::petro_chunked::_macro_util::unexpected_mini_chunk(__id)))
        };

        Ok(quote! {
            impl #impl_generics ::petro_chunked::minichunk::MiniChunkStructure<#lt> for #name #ty_generics #where_clause {
                fn collect(__mini_chunks: ::petro_chunked::minichunk::MiniChunksIter<#lt>) -> ::std::result::Result<Self, ::petro_chunked::ChunkError> {
                    #(#collectors)*
                    for __chunk in __mini_chunks {
                        let #chunk_var = __chunk?;
                        match #chunk_var.id() {
                            #(#arms)*
                            #fallback_arm
                        }
                    }
                    ::std::result::Result::Ok(Self {
                        #(#finishes),*
                    })
                }
            }
        })
    }
}

// =================================================================================================
// ChunkStructure & ChunkStreamItem
// =================================================================================================

struct ChunkContainerAttrs {
    mode: Option<ContainerOrderMode>,
    ignore_unknown: bool,
}

fn parse_chunk_container_attrs(input: &DeriveInput) -> Result<ChunkContainerAttrs> {
    let mut mode: Option<ContainerOrderMode> = None;
    let mut ignore_unknown = false;

    for attr in &input.attrs {
        if !attr.path().is_ident("chunk") {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("map") {
                if mode.is_some() {
                    return Err(meta.error("cannot specify both `map` and `sequence`"));
                }
                mode = Some(ContainerOrderMode::Map);
                Ok(())
            } else if meta.path.is_ident("sequence") {
                if mode.is_some() {
                    return Err(meta.error("cannot specify both `map` and `sequence`"));
                }
                mode = Some(ContainerOrderMode::Sequence);
                Ok(())
            } else if meta.path.is_ident("ignore_unknown") {
                ignore_unknown = true;
                Ok(())
            } else {
                Err(meta.error("unsupported container #[chunk] attribute; expected `map`, `sequence`, or `ignore_unknown`"))
            }
        })?;
    }

    Ok(ChunkContainerAttrs {
        mode,
        ignore_unknown,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChunkPayloadMode {
    Data,
    MiniChunks,
    Children,
    Ignore,
}

enum ChunkSource {
    Id { id: Expr, mode: ChunkPayloadMode },
    Stream,
}

struct ChunkFieldAttrParsed {
    source: ChunkSource,
    repeated: bool,
    optional: bool,
    default: bool,
    default_expr: Option<Expr>,
    wire_format: Option<Type>,
}

impl Parse for ChunkFieldAttrParsed {
    fn parse(input: ParseStream) -> Result<Self> {
        let is_stream = if input.peek(Ident) {
            let fork = input.fork();
            let ident: Ident = fork.parse()?;
            ident == "stream" && (fork.is_empty() || fork.peek(Token![,]))
        } else {
            false
        };

        let mut mode = ChunkPayloadMode::Data;
        let id_expr = if is_stream {
            let _: Ident = input.parse()?;
            None
        } else {
            Some(input.parse::<Expr>()?)
        };

        let mut repeated = false;
        let mut optional = false;
        let mut default = false;
        let mut default_expr = None;
        let mut wire_format = None;

        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            let kw: Ident = input.parse()?;
            if kw == "data" {
                mode = ChunkPayloadMode::Data;
            } else if kw == "mini_chunks" {
                mode = ChunkPayloadMode::MiniChunks;
            } else if kw == "children" {
                mode = ChunkPayloadMode::Children;
            } else if kw == "ignore" {
                mode = ChunkPayloadMode::Ignore;
            } else if kw == "repeated" {
                repeated = true;
            } else if kw == "optional" {
                optional = true;
            } else if kw == "required" {
                // explicit default
            } else if kw == "default" {
                if input.peek(Token![=]) {
                    let _: Token![=] = input.parse()?;
                    default_expr = Some(input.parse()?);
                } else {
                    default = true;
                }
            } else if kw == "wire_format" {
                let _: Token![=] = input.parse()?;
                wire_format = Some(input.parse()?);
            } else {
                return Err(Error::new_spanned(
                    kw,
                    "expected `data`, `mini_chunks`, `children`, `ignore`, `repeated`, `optional`, `required`, `default`, or `wire_format = ...`",
                ));
            }
        }

        let source = match id_expr {
            Some(id) => ChunkSource::Id { id, mode },
            None => ChunkSource::Stream,
        };

        Ok(Self {
            source,
            repeated,
            optional,
            default,
            default_expr,
            wire_format,
        })
    }
}

struct ChunkFieldSpec<'a> {
    ident: &'a Ident,
    ty: &'a Type,
    source: ChunkSource,
    cardinality: Cardinality<'a>,
    wire_format: Option<Type>,
}

fn parse_chunk_field<'a>(field: &'a syn::Field) -> Result<ChunkFieldSpec<'a>> {
    let ident = field
        .ident
        .as_ref()
        .ok_or_else(|| Error::new_spanned(&field.ty, "chunk struct requires named fields"))?;
    let mut parsed_attr: Option<ChunkFieldAttrParsed> = None;
    for attr in &field.attrs {
        if attr.path().is_ident("chunk") {
            if parsed_attr.is_some() {
                return Err(Error::new_spanned(
                    &attr.meta,
                    "duplicate #[chunk] attribute",
                ));
            }
            parsed_attr = Some(attr.parse_args()?);
        }
    }
    let Some(parsed) = parsed_attr else {
        return Err(Error::new_spanned(
            ident,
            "missing #[chunk(...)] attribute on field",
        ));
    };

    let ty = &field.ty;
    let cardinality = if parsed.repeated {
        let inner = extract_generic_inner(ty, "Vec").ok_or_else(|| {
            Error::new_spanned(ty, "`repeated` chunk field must have type `Vec<T>`")
        })?;
        Cardinality::Repeated(inner)
    } else if let Some(expr) = parsed.default_expr {
        Cardinality::DefaultExpr(expr)
    } else if parsed.default {
        Cardinality::Default
    } else if parsed.optional {
        let inner = extract_generic_inner(ty, "Option").ok_or_else(|| {
            Error::new_spanned(ty, "`optional` chunk field must have type `Option<T>`")
        })?;
        Cardinality::Optional(inner)
    } else {
        Cardinality::Required
    };

    Ok(ChunkFieldSpec {
        ident,
        ty,
        source: parsed.source,
        cardinality,
        wire_format: parsed.wire_format,
    })
}

fn decode_chunk_expr(
    mode: ChunkPayloadMode,
    target_ty: &Type,
    wire_format: &Option<Type>,
    chunk_ident: &Ident,
) -> TokenStream2 {
    let read_ty = wire_format.as_ref().unwrap_or(target_ty);
    let raw_read = match mode {
        ChunkPayloadMode::Data => quote!(#chunk_ident.read_data::<#read_ty>()?),
        ChunkPayloadMode::MiniChunks => quote!(#chunk_ident.read_mini_chunks::<#read_ty>()?),
        ChunkPayloadMode::Children => quote!(#chunk_ident.read_children::<#read_ty>()?),
        ChunkPayloadMode::Ignore => quote!({
            let _ = &#chunk_ident;
            <#read_ty as ::std::default::Default>::default()
        }),
    };
    if let Some(wty) = wire_format {
        quote!(<#target_ty as ::std::convert::From<#wty>>::from(#raw_read))
    } else {
        raw_read
    }
}

fn build_sequence_chunk_stmts(
    specs: &[ChunkFieldSpec<'_>],
    lt: &Lifetime,
) -> (Vec<TokenStream2>, Vec<TokenStream2>, TokenStream2) {
    let mut stmts = Vec::new();
    let mut inits = Vec::new();
    let chunk_var = format_ident!("__chunk");

    for spec in specs {
        let fident = spec.ident;
        let fty = spec.ty;

        match (&spec.source, &spec.cardinality) {
            (ChunkSource::Id { id, mode }, Cardinality::Required) => {
                let decoded = decode_chunk_expr(*mode, fty, &spec.wire_format, &chunk_var);
                stmts.push(quote! {
                    let #chunk_var = __chunks.expect_chunk(#id)?;
                    let #fident: #fty = #decoded;
                });
            }
            (ChunkSource::Id { id, mode }, Cardinality::Optional(inner)) => {
                let decoded = decode_chunk_expr(*mode, inner, &spec.wire_format, &chunk_var);
                stmts.push(quote! {
                    let #fident: #fty = match __chunks.try_expect_chunk(#id)? {
                        ::std::option::Option::Some(#chunk_var) => ::std::option::Option::Some(#decoded),
                        ::std::option::Option::None => ::std::option::Option::None,
                    };
                });
            }
            (ChunkSource::Id { id, mode }, Cardinality::Default) => {
                let decoded = decode_chunk_expr(*mode, fty, &spec.wire_format, &chunk_var);
                stmts.push(quote! {
                    let #fident: #fty = match __chunks.try_expect_chunk(#id)? {
                        ::std::option::Option::Some(#chunk_var) => #decoded,
                        ::std::option::Option::None => ::std::default::Default::default(),
                    };
                });
            }
            (ChunkSource::Id { id, mode }, Cardinality::DefaultExpr(expr)) => {
                let decoded = decode_chunk_expr(*mode, fty, &spec.wire_format, &chunk_var);
                stmts.push(quote! {
                    let #fident: #fty = match __chunks.try_expect_chunk(#id)? {
                        ::std::option::Option::Some(#chunk_var) => #decoded,
                        ::std::option::Option::None => #expr,
                    };
                });
            }
            (ChunkSource::Id { id, mode }, Cardinality::Repeated(inner)) => {
                let decoded = decode_chunk_expr(*mode, inner, &spec.wire_format, &chunk_var);
                stmts.push(quote! {
                    let mut #fident: ::std::vec::Vec<#inner> = ::std::vec::Vec::new();
                    while let ::std::option::Option::Some(#chunk_var) = __chunks.try_expect_chunk(#id)? {
                        #fident.push(#decoded);
                    }
                });
            }
            (ChunkSource::Stream, Cardinality::Required) => {
                let read_ty = spec.wire_format.as_ref().unwrap_or(fty);
                let taken = quote!(__chunks.expect_item::<#read_ty>()?);
                let converted = if let Some(wty) = &spec.wire_format {
                    quote!(<#fty as ::std::convert::From<#wty>>::from(#taken))
                } else {
                    taken
                };
                stmts.push(quote! {
                    let #fident: #fty = #converted;
                });
            }
            (ChunkSource::Stream, Cardinality::Optional(inner)) => {
                let read_ty = spec.wire_format.as_ref().unwrap_or(inner);
                let conv = if let Some(wty) = &spec.wire_format {
                    quote!(__chunks.try_expect_item::<#read_ty>()?.map(<#inner as ::std::convert::From<#wty>>::from))
                } else {
                    quote!(__chunks.try_expect_item::<#inner>()?)
                };
                stmts.push(quote! {
                    let #fident: #fty = #conv;
                });
            }
            (ChunkSource::Stream, Cardinality::Default) => {
                let read_ty = spec.wire_format.as_ref().unwrap_or(fty);
                let conv = if let Some(wty) = &spec.wire_format {
                    quote!(<#fty as ::std::convert::From<#wty>>::from(__v))
                } else {
                    quote!(__v)
                };
                stmts.push(quote! {
                    let #fident: #fty = match __chunks.try_expect_item::<#read_ty>()? {
                        ::std::option::Option::Some(__v) => #conv,
                        ::std::option::Option::None => ::std::default::Default::default(),
                    };
                });
            }
            (ChunkSource::Stream, Cardinality::DefaultExpr(expr)) => {
                let read_ty = spec.wire_format.as_ref().unwrap_or(fty);
                let conv = if let Some(wty) = &spec.wire_format {
                    quote!(<#fty as ::std::convert::From<#wty>>::from(__v))
                } else {
                    quote!(__v)
                };
                stmts.push(quote! {
                    let #fident: #fty = match __chunks.try_expect_item::<#read_ty>()? {
                        ::std::option::Option::Some(__v) => #conv,
                        ::std::option::Option::None => #expr,
                    };
                });
            }
            (ChunkSource::Stream, Cardinality::Repeated(inner)) => {
                let read_ty = spec.wire_format.as_ref().unwrap_or(inner);
                let conv = if let Some(wty) = &spec.wire_format {
                    quote!(<#inner as ::std::convert::From<#wty>>::from(__item))
                } else {
                    quote!(__item)
                };
                stmts.push(quote! {
                    let mut #fident: ::std::vec::Vec<#inner> = ::std::vec::Vec::new();
                    while let ::std::option::Option::Some(__item) = __chunks.try_expect_item::<#read_ty>()? {
                        #fident.push(#conv);
                    }
                });
            }
        }
        inits.push(quote!(#fident));
    }

    let first_match = if let Some(first) = specs.first() {
        match (&first.source, &first.cardinality) {
            (ChunkSource::Id { id, .. }, _) => quote!(__id == (#id)),
            (ChunkSource::Stream, Cardinality::Optional(inner) | Cardinality::Repeated(inner)) => {
                let read_ty = first.wire_format.as_ref().unwrap_or(inner);
                quote!(<#read_ty as ::petro_chunked::chunks::ChunkStreamItem<#lt>>::matches_start(__id))
            }
            (ChunkSource::Stream, _) => {
                let read_ty = first.wire_format.as_ref().unwrap_or(first.ty);
                quote!(<#read_ty as ::petro_chunked::chunks::ChunkStreamItem<#lt>>::matches_start(__id))
            }
        }
    } else {
        quote!(false)
    };

    (stmts, inits, first_match)
}

fn expand_chunk_structure(input: DeriveInput) -> Result<TokenStream2> {
    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "ChunkStructure can only be derived for structs (use ChunkStreamItem for chunk-tagged enums)",
        ));
    };
    let Fields::Named(named) = &data.fields else {
        return Err(Error::new_spanned(
            &input.ident,
            "ChunkStructure requires a struct with named fields",
        ));
    };

    let container_attrs = parse_chunk_container_attrs(&input)?;
    let Some(mode) = container_attrs.mode else {
        return Err(Error::new_spanned(
            &input.ident,
            "ChunkStructure requires an explicit `#[chunk(map)]` or `#[chunk(sequence)]` attribute",
        ));
    };

    let mut specs = Vec::new();
    for f in &named.named {
        specs.push(parse_chunk_field(f)?);
    }

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause, lt) =
        split_generics_with_borrow_lt(&input.generics);

    if mode == ContainerOrderMode::Sequence {
        let (stmts, inits, _) = build_sequence_chunk_stmts(&specs, &lt);

        let finish_check = if container_attrs.ignore_unknown {
            quote! {
                for __rem in __chunks {
                    let _ = __rem?;
                }
            }
        } else {
            quote! {
                __chunks.expect_empty()?;
            }
        };

        Ok(quote! {
            impl #impl_generics ::petro_chunked::chunks::ChunkStructure<#lt> for #name #ty_generics #where_clause {
                fn collect(mut __chunks: ::petro_chunked::chunks::ChunksIter<#lt>) -> ::std::result::Result<Self, ::petro_chunked::ChunkErrorV2> {
                    #(#stmts)*
                    #finish_check
                    ::std::result::Result::Ok(Self { #(#inits),* })
                }
            }
        })
    } else {
        let mut collectors = Vec::new();
        let mut stream_branches = Vec::new();
        let mut id_arms = Vec::new();
        let mut finishes = Vec::new();
        let chunk_var = format_ident!("__chunk");

        for spec in &specs {
            let fident = spec.ident;
            let fname_str = fident.to_string();
            let fty = spec.ty;

            let representative_id = match &spec.source {
                ChunkSource::Id { id, .. } => quote!(#id),
                ChunkSource::Stream => quote!(0u32),
            };

            let (collector_init, inner_ty, finish_expr) = match &spec.cardinality {
                Cardinality::Required => (
                    quote!(::petro_chunked::_macro_util::RequiredChunkEntry::<#fty>::new()),
                    fty,
                    quote!(#fident.finish(#representative_id, #fname_str)?),
                ),
                Cardinality::Optional(inner) => (
                    quote!(::petro_chunked::_macro_util::OptionalChunkEntry::<#inner>::new()),
                    *inner,
                    quote!(#fident.finish(#representative_id, #fname_str)?),
                ),
                Cardinality::Default => (
                    quote!(::petro_chunked::_macro_util::OptionalChunkEntry::<#fty>::new()),
                    fty,
                    quote!(#fident.finish_or_default()?),
                ),
                Cardinality::DefaultExpr(expr) => (
                    quote!(::petro_chunked::_macro_util::OptionalChunkEntry::<#fty>::new()),
                    fty,
                    quote!(#fident.finish_or_else(|| #expr)?),
                ),
                Cardinality::Repeated(inner) => (
                    quote!(::petro_chunked::_macro_util::RepeatedChunkEntry::<#inner>::new()),
                    *inner,
                    quote!(#fident.finish(#representative_id, #fname_str)?),
                ),
            };

            collectors.push(quote! {
                let mut #fident = #collector_init;
            });

            match &spec.source {
                ChunkSource::Id { id, mode } => {
                    let decoded = decode_chunk_expr(*mode, inner_ty, &spec.wire_format, &chunk_var);
                    id_arms.push(quote! {
                        __id if __id == (#id) => {
                            #fident.add_value(__id, #fname_str, #decoded)?;
                        }
                    });
                }
                ChunkSource::Stream => {
                    let read_ty = spec.wire_format.as_ref().unwrap_or(inner_ty);
                    let taken = quote!(<#read_ty as ::petro_chunked::chunks::ChunkStreamItem<#lt>>::take_from_chunks(&mut __chunks)?);
                    let converted = if let Some(wty) = &spec.wire_format {
                        quote!(<#inner_ty as ::std::convert::From<#wty>>::from(#taken))
                    } else {
                        taken
                    };
                    stream_branches.push(quote! {
                        if <#read_ty as ::petro_chunked::chunks::ChunkStreamItem<#lt>>::matches_start(__peek_id) {
                            let __val = #converted;
                            #fident.add_value(__peek_id, #fname_str, __val)?;
                            continue;
                        }
                    });
                }
            }

            finishes.push(quote! {
                #fident: #finish_expr
            });
        }

        let fallback_arm = if container_attrs.ignore_unknown {
            quote!(_ => {})
        } else {
            quote!(__id => return ::std::result::Result::Err(::petro_chunked::_macro_util::unexpected_chunk(__id)))
        };

        Ok(quote! {
            impl #impl_generics ::petro_chunked::chunks::ChunkStructure<#lt> for #name #ty_generics #where_clause {
                fn collect(mut __chunks: ::petro_chunked::chunks::ChunksIter<#lt>) -> ::std::result::Result<Self, ::petro_chunked::ChunkErrorV2> {
                    #(#collectors)*
                    while let ::std::option::Option::Some(__peek_id) = __chunks.peek_id()? {
                        #(#stream_branches)*
                        let #chunk_var = __chunks.next().unwrap()?;
                        match #chunk_var.id() {
                            #(#id_arms)*
                            #fallback_arm
                        }
                    }
                    ::std::result::Result::Ok(Self {
                        #(#finishes),*
                    })
                }
            }
        })
    }
}

fn expand_chunk_stream_item(input: DeriveInput) -> Result<TokenStream2> {
    match &input.data {
        Data::Enum(data) => expand_chunk_stream_item_enum(&input, data),
        Data::Struct(data) => expand_chunk_stream_item_struct(&input, data),
        Data::Union(_) => Err(Error::new_spanned(
            &input.ident,
            "ChunkStreamItem cannot be derived for unions",
        )),
    }
}

fn expand_chunk_stream_item_struct(
    input: &DeriveInput,
    data: &syn::DataStruct,
) -> Result<TokenStream2> {
    let Fields::Named(named) = &data.fields else {
        return Err(Error::new_spanned(
            &input.ident,
            "ChunkStreamItem struct requires named fields",
        ));
    };

    let container_attrs = parse_chunk_container_attrs(input)?;
    if container_attrs.mode == Some(ContainerOrderMode::Map) {
        return Err(Error::new_spanned(
            &input.ident,
            "ChunkStreamItem cannot be derived for `#[chunk(map)]` structs; stream items must be sequential",
        ));
    }

    let mut specs = Vec::new();
    for f in &named.named {
        specs.push(parse_chunk_field(f)?);
    }

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause, lt) =
        split_generics_with_borrow_lt(&input.generics);

    let (stmts, inits, first_match) = build_sequence_chunk_stmts(&specs, &lt);

    Ok(quote! {
        impl #impl_generics ::petro_chunked::chunks::ChunkStreamItem<#lt> for #name #ty_generics #where_clause {
            fn matches_start(__id: u32) -> bool {
                #first_match
            }

            fn take_from_chunks(__chunks: &mut ::petro_chunked::chunks::ChunksIter<#lt>) -> ::std::result::Result<Self, ::petro_chunked::ChunkErrorV2> {
                #(#stmts)*
                ::std::result::Result::Ok(Self { #(#inits),* })
            }
        }
    })
}

fn expand_chunk_stream_item_enum(
    input: &DeriveInput,
    data: &syn::DataEnum,
) -> Result<TokenStream2> {
    if data.variants.is_empty() {
        return Err(Error::new_spanned(
            &input.ident,
            "ChunkStreamItem enum must have at least one variant",
        ));
    }

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause, lt) =
        split_generics_with_borrow_lt(&input.generics);

    let mut match_checks = Vec::new();
    let mut parse_arms = Vec::new();

    for variant in &data.variants {
        let vident = &variant.ident;
        let mut parsed_attr: Option<ChunkFieldAttrParsed> = None;
        for attr in &variant.attrs {
            if attr.path().is_ident("chunk") {
                if parsed_attr.is_some() {
                    return Err(Error::new_spanned(
                        &attr.meta,
                        "duplicate #[chunk] attribute",
                    ));
                }
                parsed_attr = Some(attr.parse_args()?);
            }
        }
        let Some(parsed) = parsed_attr else {
            return Err(Error::new_spanned(
                vident,
                "missing #[chunk(<id>)] attribute on enum variant",
            ));
        };

        match parsed.source {
            ChunkSource::Id { id, mode } => {
                match_checks.push(quote!(__id == (#id)));
                let construct = match &variant.fields {
                    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                        let inner_ty = &fields.unnamed.first().unwrap().ty;
                        let chunk_var = format_ident!("__chunk");
                        let decoded =
                            decode_chunk_expr(mode, inner_ty, &parsed.wire_format, &chunk_var);
                        quote! {
                            let __chunk = __chunks.expect_chunk(#id)?;
                            ::std::result::Result::Ok(Self::#vident(#decoded))
                        }
                    }
                    Fields::Unit => {
                        quote! {
                            let _ = __chunks.expect_chunk(#id)?;
                            ::std::result::Result::Ok(Self::#vident)
                        }
                    }
                    _ => {
                        return Err(Error::new_spanned(
                            vident,
                            "ChunkStreamItem enum variants must be a 1-element tuple variant `Variant(T)` or a unit variant `Variant`",
                        ));
                    }
                };
                parse_arms.push(quote! {
                    __id if __id == (#id) => {
                        #construct
                    }
                });
            }
            ChunkSource::Stream => {
                let Fields::Unnamed(fields) = &variant.fields else {
                    return Err(Error::new_spanned(
                        vident,
                        "`#[chunk(stream)]` enum variant must be a 1-element tuple variant `Variant(T)`",
                    ));
                };
                if fields.unnamed.len() != 1 {
                    return Err(Error::new_spanned(
                        vident,
                        "`#[chunk(stream)]` enum variant must be a 1-element tuple variant `Variant(T)`",
                    ));
                }
                let inner_ty = &fields.unnamed.first().unwrap().ty;
                let read_ty = parsed.wire_format.as_ref().unwrap_or(inner_ty);
                match_checks.push(
                    quote!(<#read_ty as ::petro_chunked::chunks::ChunkStreamItem<#lt>>::matches_start(__id)),
                );
                let taken = quote!(<#read_ty as ::petro_chunked::chunks::ChunkStreamItem<#lt>>::take_from_chunks(__chunks)?);
                let converted = if let Some(wty) = &parsed.wire_format {
                    quote!(<#inner_ty as ::std::convert::From<#wty>>::from(#taken))
                } else {
                    taken
                };
                parse_arms.push(quote! {
                    __id if <#read_ty as ::petro_chunked::chunks::ChunkStreamItem<#lt>>::matches_start(__id) => {
                        ::std::result::Result::Ok(Self::#vident(#converted))
                    }
                });
            }
        }
    }

    Ok(quote! {
        impl #impl_generics ::petro_chunked::chunks::ChunkStreamItem<#lt> for #name #ty_generics #where_clause {
            fn matches_start(__id: u32) -> bool {
                #(#match_checks)||*
            }

            fn take_from_chunks(__chunks: &mut ::petro_chunked::chunks::ChunksIter<#lt>) -> ::std::result::Result<Self, ::petro_chunked::ChunkErrorV2> {
                let Some(__peek_id) = __chunks.peek_id()? else {
                    return ::std::result::Result::Err(::petro_chunked::ChunkError::EndOfChunks.into());
                };
                match __peek_id {
                    #(#parse_arms)*
                    __id => ::std::result::Result::Err(::petro_chunked::_macro_util::unexpected_chunk(__id)),
                }
            }
        }
    })
}
