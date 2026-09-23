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
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, Item, ItemImpl, Result, parse2};

use super::Arity;
use super::ext::{AttributesExt, FieldExt, TypeExt};

/// A grammar node, as declared.
///
/// NOTE(#syntax-derive/grammar-is-a-type): V[S(Grammar).M(node) && S(Grammar).M(reader)], "The
/// three builders were free functions over `&[Field]` plus whichever other argument each needed -
/// `node_const(entry, fields)`, `reader(name, fields)`, `shape_bounds(fields)`. Three functions
/// sharing a parameter list IS a type, and writing it down means the entry name and the fields
/// cannot be passed in the wrong order or forgotten. ID(derive/helpers-belong-to-types)"
struct Grammar<'ast> {
    /// The type being derived on.
    name: &'ast syn::Ident,
    /// Its entry attribute head - the type name in snake_case, via heck at expansion time.
    entry: String,
    fields: Vec<Field<'ast>>,
    generics: &'ast syn::Generics,
}

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

pub(crate) fn derive_syntax(input: DeriveInput) -> Result<Vec<Item>> {
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

    let grammar = Grammar {
        name,
        // heck at EXPANSION time, so the entry head is a literal and matching stays exact -
        entry: name.to_string().to_snake_case(),
        fields: named.named.iter().map(Field::read).collect::<Result<_>>()?,
        generics: &input.generics,
    };

    let (impl_generics, type_generics, where_clause) = grammar.generics.split_for_impl();

    let node = grammar.node();
    let reader = grammar.reader();
    let bounds = grammar.bounds()?;

    // Each item parsed on its own, so a malformed one names the generator that built it rather
    // than arriving in the author's crate - NOTE(#derive/expansion-is-typed-items).
    let described = parse2::<ItemImpl>(quote! {
        impl #impl_generics ::proc_macro_flow_traits::node::Described
            for #name #type_generics #where_clause
        {
            #node
        }
    })?;

    let reader = parse2::<ItemImpl>(quote! {
        impl #impl_generics ::proc_macro_flow_traits::vocab::leaves::FromMeta
            for #name #type_generics #where_clause
        {
            #reader
        }
    })?;

    let mut items = vec![Item::Impl(described), Item::Impl(reader)];
    items.extend(bounds);
    Ok(items)
}

/// Read one field's declaration.
impl<'ast> Field<'ast> {
    /// Read one field's declaration.
    fn read(field: &'ast syn::Field) -> Result<Self> {
        let ident = field.named_ident()?;
        let canonical = ident.to_string().to_snake_case();

        // `#[alias]` with no arguments asks for the standard case set; `#[alias("x", "y")]` adds
        // exactly what it names. Both emit LITERALS, so matching stays exact per ID(vocabulary/exact) -
        // the generation is a convention, not a normalisation rule applied at match time.
        let aliases = match field.attrs.find_one("alias")? {
            None => Vec::new(),
            Some(attr) if matches!(attr.meta, syn::Meta::Path(_)) => {
                Field::standard_cases(&canonical)
            }
            Some(attr) => attr
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated,
                )?
                .into_iter()
                .map(|lit| lit.value())
                .collect(),
        };

        let shape = match field.attrs.find_one("shape")? {
            Some(attr) => Some(attr.parse_args::<syn::Path>()?),
            None => None,
        };

        Ok(Field {
            ident,
            ty: &field.ty,
            key: canonical,
            aliases,
            arity: field.ty.arity(),
            shape,
        })
    }
}

/// The standard alias set: the same name in the three casings a user might reach for.
///
/// Canonical is excluded - it is already `key`, and a spelling listed twice would show up twice in
/// a did-you-mean list.
impl Field<'_> {
    fn standard_cases(canonical: &str) -> Vec<String> {
        [canonical.to_lower_camel_case(), canonical.to_kebab_case()]
            .into_iter()
            .filter(|spelling| spelling != canonical)
            .collect()
    }
}

impl<'ast> Grammar<'ast> {
    /// The reflection table this node emits.
    fn node(&self) -> TokenStream {
        let (entry, fields) = (&self.entry, &self.fields);
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
    /// The `from_meta` body: walk the list, read each field, then check what was required.
    fn reader(&self) -> TokenStream {
        let (name, fields) = (self.name, &self.fields);
        let idents: Vec<&syn::Ident> = fields.iter().map(|field| field.ident).collect();
        let keys: Vec<&String> = fields.iter().map(|field| &field.key).collect();
        let aliases = fields.iter().map(|field| &field.aliases);

        let reads = fields.iter().map(|field| {
            let ident = field.ident;
            let inner = field.ty.inner();
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

        // EVERY missing key is reported, not just the first. The earlier shape returned as soon as it
        // found one, which is the sibling-dropping ID(no-result) exists to prevent - and it reached
        // for `.err().expect("not empty")` to do it, a panic in the AUTHOR'S compile standing on an
        // invariant established two lines away.
        let missing = fields
            .iter()
            .filter(|field| field.arity != Arity::Maybe)
            .map(|field| {
                let ident = field.ident;
                let key = &field.key;
                quote! {
                    if #ident.is_none() {
                        errors.push(::syn::Error::new_spanned(
                            meta,
                            ::std::concat!("missing required key `", #key, "`"),
                        ));
                    }
                }
            });

        let takes = fields.iter().map(|field| {
            let ident = field.ident;
            let key = &field.key;
            match field.arity {
                Arity::Maybe => quote!( #ident: #ident ),
                // Reached only inside the Ok arm, where the check above has already passed - so None
                // would be a FRAMEWORK bug. It bubbles a diagnostic saying so rather than panicking;
                // see NOTE(#derive/no-panics).
                _ => quote! {
                    #ident: match #ident {
                        ::std::option::Option::Some(value) => value,
                        ::std::option::Option::None => {
                            return ::std::result::Result::Err(::syn::Error::new_spanned(
                                meta,
                                ::std::concat!(
                                    "internal: `", #key, "` passed the required check and then was \
                                     not present. This is a proc_macro_flow bug."
                                ),
                            ));
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

                #(#missing)*

                match errors.finish() {
                    ::std::result::Result::Err(error) => ::std::result::Result::Err(error),
                    ::std::result::Result::Ok(()) => {
                        ::std::result::Result::Ok(#name { #(#takes),* })
                    }
                }
            }
        }
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
    /// Step 6: the selector becomes a BOUND.
    fn bounds(&self) -> Result<Vec<Item>> {
        let fields = &self.fields;
        let assertions = fields.iter().filter_map(|field| {
        let path = field.shape.as_ref()?;
        let inner = field.ty.inner();
        let span = path.segments.last().map(|s| s.ident.span())?;

        Some(parse2::<Item>(quote::quote_spanned! { span =>
            const _: () = {
                const fn assert_shape<S: ::proc_macro_flow_traits::meta::Shape>() {}
                assert_shape::<#path>();
                const fn assert_readable<T: ::proc_macro_flow_traits::vocab::leaves::FromMeta>() {}
                assert_readable::<#inner>();
            };
        }))
    });

        assertions.collect()
    }
}
