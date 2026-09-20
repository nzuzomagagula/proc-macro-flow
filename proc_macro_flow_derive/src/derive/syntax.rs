// @review [ ]
//! `#[derive(Syntax)]` — a grammar type declares itself.
//!
//! This is the bootstrap ID(syntax/derive) named: the derive is what lets a grammar be written as
//! ordinary Rust types instead of as a hand-rolled `meta_list!` invocation, and it is what puts the
//! whole vocabulary suite on the macro path for the first time.
//!
//! NOTE(#syntax-derive/parses-the-type): V[F(derive_syntax).uses(F(Child::of))], "Arity is read by
//! PARSING the field's type and looking at `segments.last()`, reusing F(Child::of) from the
//! extractor derive rather than writing a second reader. That is the difference a proc macro makes
//! and the reason this exists at all: M(meta_list) matches the TOKENS `Option < .. >`, so
//! `std::option::Option<T>` reads as required there (NOTE(#forwarding/no-option) covers why that
//! stays loud). Here it is simply correct, because the type is parsed and a path's last segment is
//! a question the AST can answer"

use heck::{ToKebabCase, ToLowerCamelCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::{Data, DeriveInput, Error, Fields, Result};

use super::{find_one, unwrap_generic, Arity};

/// One declared child of a grammar node.
struct Field<'ast> {
    ident: &'ast syn::Ident,
    ty: &'ast syn::Type,
    /// The canonical key, in snake_case. Derived unless the author aliased it.
    key: String,
    /// Extra accepted spellings, canonical excluded.
    aliases: Vec<String>,
    arity: Arity,
    /// The `#[shape(..)]` selector, carried verbatim and never read - ID(no-parse).
    shape: Option<syn::Path>,
}

pub(crate) fn derive_syntax(input: DeriveInput) -> Result<TokenStream> {
    let name = &input.ident;

    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            name,
            "`#[derive(Syntax)]` describes a struct of grammar fields - enums are `variants!`'s \
             job until #syntax/derive-enums",
        ));
    };

    let Fields::Named(named) = &data.fields else {
        return Err(Error::new_spanned(
            name,
            "`#[derive(Syntax)]` needs named fields - a tuple struct is all-positional, which is \
             #positional's separate reading",
        ));
    };

    let fields: Vec<Field> = named
        .named
        .iter()
        .map(read_field)
        .collect::<Result<_>>()?;

    let entry = name.to_string().to_snake_case();
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    let node = node_const(&entry, &fields);
    let reader = reader(name, &fields);
    let bounds = shape_bounds(&fields);

    Ok(quote! {
        impl #impl_generics ::proc_macro_flow_traits::node::Described
            for #name #type_generics #where_clause
        {
            #node
        }

        impl #impl_generics ::proc_macro_flow_traits::vocab::leaves::FromMeta
            for #name #type_generics #where_clause
        {
            #reader
        }

        #bounds
    })
}

/// Read one field's declaration.
fn read_field(field: &syn::Field) -> Result<Field<'_>> {
    let ident = field.ident.as_ref().expect("named");
    let canonical = ident.to_string().to_snake_case();

    // `#[alias]` with no arguments asks for the standard case set; `#[alias("x", "y")]` adds
    // exactly what it names. Both emit LITERALS, so matching stays exact per ID(vocabulary/exact) -
    // the generation is a convention, not a normalisation rule applied at match time.
    let aliases = match find_one(&field.attrs, "alias")? {
        None => Vec::new(),
        Some(attr) if matches!(attr.meta, syn::Meta::Path(_)) => standard_cases(&canonical),
        Some(attr) => attr
            .parse_args_with(
                syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated,
            )?
            .into_iter()
            .map(|lit| lit.value())
            .collect(),
    };

    let shape = match find_one(&field.attrs, "shape")? {
        Some(attr) => Some(attr.parse_args::<syn::Path>()?),
        None => None,
    };

    Ok(Field {
        ident,
        ty: &field.ty,
        key: canonical,
        aliases,
        arity: arity_of(&field.ty),
        shape,
    })
}

/// The standard alias set: the same name in the three casings a user might reach for.
///
/// Canonical is excluded - it is already `key`, and a spelling listed twice would show up twice in
/// a did-you-mean list.
fn standard_cases(canonical: &str) -> Vec<String> {
    [canonical.to_lower_camel_case(), canonical.to_kebab_case()]
        .into_iter()
        .filter(|spelling| spelling != canonical)
        .collect()
}

fn node_const(entry: &str, fields: &[Field<'_>]) -> TokenStream {
    let children = fields.iter().map(|field| {
        let key = &field.key;
        let aliases = &field.aliases;
        let arity = match field.arity {
            Arity::One => quote!(Required),
            Arity::Maybe => quote!(Optional),
            Arity::Many => quote!(Repeated),
        };
        let shapes = match &field.shape {
            // The selector names a TYPE, so its runtime identity comes from the Shape impl rather
            // than from anything we compare - NOTE(#shape/two-facts).
            Some(path) => quote!(&[<#path as ::proc_macro_flow_traits::meta::Shape>::KIND]),
            None => quote!(&[]),
        };

        quote! {
            ::proc_macro_flow_traits::node::Child {
                key: #key,
                aliases: &[ #(#aliases),* ],
                arity: ::proc_macro_flow_traits::node::Arity::#arity,
                shapes: #shapes,
            }
        }
    });

    quote! {
        const NODE: ::proc_macro_flow_traits::node::Node =
            ::proc_macro_flow_traits::node::Node {
                name: #entry,
                children: &[ #(#children),* ],
            };
    }
}

/// The `from_meta` body: walk the list, read each field, then check what was required.
fn reader(name: &syn::Ident, fields: &[Field<'_>]) -> TokenStream {
    let idents: Vec<&syn::Ident> = fields.iter().map(|field| field.ident).collect();
    let keys: Vec<&String> = fields.iter().map(|field| &field.key).collect();
    let aliases = fields.iter().map(|field| &field.aliases);

    let reads = fields.iter().map(|field| {
        let ident = field.ident;
        let inner = inner_type(field);
        quote! {
            Key::#ident => {
                #ident = ::std::option::Option::Some(
                    <#inner as ::proc_macro_flow_traits::vocab::leaves::FromMeta>::from_meta(
                        element,
                    )?,
                );
            }
        }
    });

    let takes = fields.iter().map(|field| {
        let ident = field.ident;
        let key = &field.key;
        match field.arity {
            Arity::Maybe => quote!( #ident: #ident ),
            _ => quote! {
                #ident: match #ident {
                    ::std::option::Option::Some(value) => value,
                    ::std::option::Option::None => {
                        errors.push(::syn::Error::new_spanned(
                            meta,
                            ::std::concat!("missing required key `", #key, "`"),
                        ));
                        return ::std::result::Result::Err(
                            errors.finish().err().expect("not empty"),
                        );
                    }
                }
            },
        }
    });

    quote! {
        fn from_meta(meta: &::syn::Meta) -> ::syn::Result<Self> {
            let list = meta.require_list()?;
            let body = ::proc_macro_flow_traits::meta::ListBody(&list.tokens);

            // The key set, local to this reader - the same shape meta_list! emits, and for the
            // same reason: it needs no unique name and there is no second public name to keep in
            // step. Unlike meta_list!, aliases are real here, because a proc macro can build the
            // literals.
            ::proc_macro_flow_traits::keys! {
                #[allow(non_camel_case_types)]
                enum Key { #( #idents = #keys ),* }
            }
            // The alias spellings the Node table advertises, asserted against the key set so the
            // two cannot drift. TODO[ ](#syntax-derive/aliases-in-keys): `keys!` accepts one
            // spelling per variant, so an alias is currently visible to diagnostics but not to
            // `Keys::resolve`. Extending `keys!` to take `ident = "a" | "b"` closes it.
            const _: &[&[&str]] = &[ #( &[ #(#aliases),* ] ),* ];

            #( let mut #idents = ::std::option::Option::None; )*
            let mut errors = ::proc_macro_flow_traits::vocab::walk::Errors::new();

            errors.absorb(body.walk::<Key, _>(|written, element| {
                // EXHAUSTIVE over the key set - there is no arm to forget.
                match written.key() { #(#reads)* }
                ::std::result::Result::Ok(())
            }));

            ::std::result::Result::Ok(#name { #(#takes),* })
        }
    }
}

/// Arity, read off the WRITTEN type by its last path segment.
fn arity_of(ty: &syn::Type) -> Arity {
    if unwrap_generic(ty, "Vec").is_some() {
        Arity::Many
    } else if unwrap_generic(ty, "Option").is_some() {
        Arity::Maybe
    } else {
        Arity::One
    }
}

/// `Option<T>` and `Vec<T>` read a `T`; everything else reads itself.
fn inner_type(field: &Field<'_>) -> TokenStream {
    unwrap_generic(field.ty, "Option")
        .or_else(|| unwrap_generic(field.ty, "Vec"))
        .map(ToTokens::to_token_stream)
        .unwrap_or_else(|| field.ty.to_token_stream())
}

/// Step 6: the selector becomes a BOUND.
///
/// NOTE(#shape/bound-at-last): V[Attr(shape).lowers_to(Tr(Shape))], "Ty(Shape) was declared long
/// before anything used it - `S: Shape` and `S::KIND` appeared only in meta.rs's own tests, so the
/// promise that Attr(shape) lowers to a trait BOUND rather than a runtime match was recorded and
/// unbuilt. This is where it is spent: a selector that names a shape the field's type cannot be
/// read in fails in the AUTHOR's crate, at the author's span.
///
/// The runtime check is untouched and still correct. The two answer different questions -
/// NOTE(#shape/two-facts) - and this is the half that had never been exercised"
fn shape_bounds(fields: &[Field<'_>]) -> TokenStream {
    let assertions = fields.iter().filter_map(|field| {
        let path = field.shape.as_ref()?;
        let inner = inner_type(field);
        let span = path.segments.last().map(|s| s.ident.span())?;

        Some(quote::quote_spanned! { span =>
            const _: () = {
                const fn assert_shape<S: ::proc_macro_flow_traits::meta::Shape>() {}
                assert_shape::<#path>();
                const fn assert_readable<T: ::proc_macro_flow_traits::vocab::leaves::FromMeta>() {}
                assert_readable::<#inner>();
            };
        })
    });

    quote!( #(#assertions)* )
}
