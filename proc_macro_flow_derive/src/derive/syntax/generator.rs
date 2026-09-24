// @review [ ]
//! What `#[derive(Syntax)]` builds: four impls and the shape bounds.

use heck::ToUpperCamelCase;
use proc_macro_flow_traits::assert::AssertKind;
use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use quote::{quote, ToTokens};
use syn::{parse2, DeriveInput, Error, ImplItem, Item, ItemImpl, Result};

use super::super::Arity;
use super::super::ext::TypeExt;
use super::processor::{Field, Grammar, Rule};

/// Four impls and the shape bounds.
pub(crate) struct SyntaxExpansion(ItemImpl, ItemImpl, ItemImpl, ItemImpl, Bounds);

/// One `const _` per `#[shape(..)]` selector - however many the grammar declared.
///
/// A newtype over a Ty(Vec) rather than a counted tuple, and honestly so: the bound SET is one
/// thing whose size the grammar decides, unlike the four impls above, which are always four.
pub(crate) struct Bounds(Vec<Item>);

impl ToTokens for SyntaxExpansion {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
        self.1.to_tokens(tokens);
        self.2.to_tokens(tokens);
        self.3.to_tokens(tokens);
        self.4.to_tokens(tokens);
    }
}

impl ToTokens for Bounds {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        for item in &self.0 {
            item.to_tokens(tokens);
        }
    }
}

impl<'ast> Generator<'ast> for SyntaxExpansion {
    type Input = Grammar<'ast>;
    type Subject = &'ast DeriveInput;
    type Output = Self;

    fn generate(grammar: Grammar<'ast>) -> Extraction<Self> {
        let name = grammar.name;
        let (impl_generics, type_generics, where_clause) = grammar.generics.split_for_impl();

        let built = (|| -> Result<SyntaxExpansion> {
            let node = grammar.node()?;
            let reader = grammar.reader()?;
            let assert = grammar.assert()?;
            let bounds = grammar.bounds()?;

            // Each item parsed on its own, so a malformed one names the generator that built it
            // rather than arriving in the author's crate - NOTE(#derive/expansion-is-typed-items).
            let described = parse2::<ItemImpl>(quote! {
                impl #impl_generics ::proc_macro_flow_traits::node::Described
                    for #name #type_generics #where_clause
                {
                    #node
                }
            })?;

            // The WORK. See NOTE(#from-body/one-reader-two-entries) for why this is the half that
            // holds it: the reader never wanted the attribute's head, only the tokens inside its
            // delimiters.
            let body = parse2::<ItemImpl>(quote! {
                impl #impl_generics ::proc_macro_flow_traits::vocab::leaves::FromBody
                    for #name #type_generics #where_clause
                {
                    #reader
                }
            })?;

            // The ADAPTER, and the only place `require_list` survives. `meta` is passed as the
            // fallback rather than `meta.span()`, so every span on this path is what it was before
            // the split - NOTE(#from-body/fallback-is-tokens-not-a-span).
            let from_meta = parse2::<ItemImpl>(quote! {
                impl #impl_generics ::proc_macro_flow_traits::vocab::leaves::FromMeta
                    for #name #type_generics #where_clause
                {
                    fn from_meta(meta: &::proc_macro_flow_traits::syn::Meta) -> ::proc_macro_flow_traits::syn::Result<Self> {
                        let list = ::proc_macro_flow_traits::syn::Meta::require_list(meta)?;
                        <Self as ::proc_macro_flow_traits::vocab::leaves::FromBody>::from_body(
                            &list.tokens,
                            meta,
                        )
                    }
                }
            })?;

            let asserts = parse2::<ItemImpl>(quote! {
                impl #impl_generics ::proc_macro_flow_traits::assert::Assert
                    for #name #type_generics #where_clause
                {
                    #assert
                }
            })?;

            Ok(SyntaxExpansion(
                described,
                body,
                from_meta,
                asserts,
                Bounds(bounds),
            ))
        })();

        match built {
            Ok(expansion) => Extraction::value(expansion),
            Err(error) => Extraction::failed(Reason::new(ReasonKind::Internal(error))),
        }
    }

    /// No vacant form - NOTE(#derive/the-impl-is-the-product).
    fn stub(subject: &'ast DeriveInput) -> Result<Self> {
        Err(Error::new_spanned(
            &subject.ident,
            "`#[derive(Syntax)]` describes a struct of NAMED grammar fields - enums are \
             `variants!`'s job until #syntax/derive-enums, and a tuple struct is all-positional",
        ))
    }
}


impl<'ast> Grammar<'ast> {
    /// The reflection table this node emits.
    fn node(&self) -> Result<ImplItem> {
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

        parse2(quote! {
            const NODE: ::proc_macro_flow_traits::node::Node =
                ::proc_macro_flow_traits::node::Node {
                    name: #entry,
                    children: &[ #(#children),* ],
                };
        })
    }

    /// The `from_body` body: walk the list, read each field, then check what was required.
    ///
    /// Everything a complaint here cannot span itself falls back to `at`, which is `meta` on the
    /// derive path and the ANNOTATED ITEM on the attribute-macro path - see
    /// NOTE(#from-body/fallback-is-tokens-not-a-span).
    fn reader(&self) -> Result<ImplItem> {
        let (name, fields) = (self.name, &self.fields);
        let key_set = quote::format_ident!("__{}Keys", name);
        let idents: Vec<&syn::Ident> = fields.iter().map(|field| field.ident).collect();
        let keys: Vec<&String> = fields.iter().map(|field| &field.key).collect();
        let aliases = fields.iter().map(|field| &field.aliases);

        let reads = fields.iter().map(|field| {
            let ident = field.ident;
            let inner = field.ty.inner();
            quote! {
                #key_set::#ident => {
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
                        errors.push(::proc_macro_flow_traits::syn::Error::new_spanned(
                            at,
                            ::std::concat!("missing required key `", #key, "`"),
                        ));
                    }
                }
            });

        let takes = fields.iter().map(|field| {
            let ident = field.ident;
            let key = &field.key;
            match field.arity {
                // Field-init SHORTHAND, not `#ident: #ident`. The long form is what
                // `clippy::redundant_field_names` fires on, and a lint in generated code is
                // reported against the AUTHOR's struct - they see a warning about a line they
                // did not write and cannot silence. Generated code owes the same cleanliness as
                // written code; see NOTE(#derive/no-panics) for the same argument about panics.
                Arity::Maybe => quote!( #ident ),
                // Reached only inside the Ok arm, where the check above has already passed - so None
                // would be a FRAMEWORK bug. It bubbles a diagnostic saying so rather than panicking;
                // see NOTE(#derive/no-panics).
                _ => quote! {
                    #ident: match #ident {
                        ::std::option::Option::Some(value) => value,
                        ::std::option::Option::None => {
                            return ::std::result::Result::Err(::proc_macro_flow_traits::syn::Error::new_spanned(
                                at,
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

        parse2(quote! {
            fn from_body<__At>(
                body: &::proc_macro_flow_traits::proc_macro2::TokenStream,
                at: &__At,
            ) -> ::proc_macro_flow_traits::syn::Result<Self>
            where
                __At: ::proc_macro_flow_traits::quote::ToTokens + ?::std::marker::Sized,
            {
                let body = ::proc_macro_flow_traits::meta::ListBody(body);

                // The key set, local to this reader - the same shape meta_list! emits, and for the
                // same reason: it needs no unique name and there is no second public name to keep in
                // step. Unlike meta_list!, aliases are real here, because a proc macro can build the
                // literals.
                // TODO[x](#syntax/key-set-shadowing): U[E(keys).name], "The generated key enum
                // was called `Key` and shadowed any author type of that name, failing with a path
                // nobody wrote"
                // NOTE(#syntax-derive/the-key-set-cannot-shadow): V[E(keys).name.derived], "Named
                // after the grammar rather than `Key`, because this enum is declared INSIDE the
                // reader's body and a bare `Key` shadows any type the author happens to have called
                // that - including one used as a field's own type in this very grammar. The failure
                // reads `the trait bound <Column as FromBody>::from_body::Key: FromMeta is not
                // satisfied`, which names a path the author never wrote."
                ::proc_macro_flow_traits::keys! {
                    #[allow(non_camel_case_types)]
                    enum #key_set { #( #idents = #keys ),* }
                }
                // The alias spellings the Node table advertises, asserted against the key set so the
                // two cannot drift. TODO[ ](#syntax-derive/aliases-in-keys): `keys!` accepts one
                // spelling per variant, so an alias is currently visible to diagnostics but not to
                // `Keys::resolve`. Extending `keys!` to take `ident = "a" | "b"` closes it.
                const _: &[&[&str]] = &[ #( &[ #(#aliases),* ] ),* ];

                #( let mut #idents = ::std::option::Option::None; )*
                let mut errors = ::proc_macro_flow_traits::vocab::walk::Errors::new();

                errors.absorb(body.walk::<#key_set, _>(|written, element| {
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
        })
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

// TODO[x](#assert/derive-reads-rules): C[Attr(assert)] && V[F(Rule::resolve).rejects(required)],
// "Attr(assert) read off the type, with three checks a derive can make and a runtime cannot: the
// field exists, the rule takes that many keys, and - the one that earns it - a REQUIRED field is
// always written, so a rule asking whether it was is a statement its own type contradicts"

impl Grammar<'_> {
    /// The `assert` body: this grammar's own rules, then a descent into every field.
    ///
    /// The descent is unconditional and needs no knowledge of which fields are grammars, because
    /// every leaf is askable too - NOTE(#assert/leaves-are-askable).
    fn assert(&self) -> Result<ImplItem> {
        let checks = self
            .rules
            .iter()
            .map(|rule| rule.emit(&self.fields))
            .collect::<Result<Vec<syn::Stmt>>>()?;
        let descend = self.fields.iter().map(|field| {
            let ident = field.ident;
            quote!(::proc_macro_flow_traits::assert::Assert::assert(&self.#ident, out);)
        });

        parse2(quote! {
            fn assert(&self, out: &mut ::std::vec::Vec<::proc_macro_flow_traits::extractor::Reason>) {
                #(#checks)*
                #(#descend)*
            }
        })
    }
}

impl Field<'_> {
    /// Whether this key was WRITTEN, read off the arity the type already stated.
    ///
    /// `Arity::One` never reaches here - a required key is always written, and
    /// ID(assert/rules-are-checked-at-derive-time) rejects a rule naming one.
    fn was_written(&self) -> Result<syn::Expr> {
        let ident = self.ident;
        parse2(match self.arity {
            Arity::Maybe => quote!(::std::option::Option::is_some(&self.#ident)),
            Arity::Many => quote!(!::std::vec::Vec::is_empty(&self.#ident)),
            Arity::One => quote!(true),
        })
    }
}

impl Rule<'_> {
    /// The check itself.
    ///
    /// Each one records a E(Reason) and carries on - there is no `?` and no early return, so a
    /// grammar stating three rules reports all three it breaks rather than the first
    /// (NOTE(#assert/no-result)).
    fn emit(&self, fields: &[Field<'_>]) -> Result<syn::Stmt> {
        let (kind, named, _) = match self {
            // An author's rule words its OWN reason, so nothing is built here -
            // NOTE(#assert/with-never-violates).
            Rule::With(path) => {
                return parse2(quote! {
                    <#path as ::proc_macro_flow_traits::assert::Rule>::check(self, out);
                });
            }
            Rule::Builtin { kind, fields, head } => (kind, fields, head),
        };

        // Resolved when the rule was read, so a miss here cannot happen on a grammar that got
        // this far - and it still bubbles rather than panicking (NOTE(#derive/no-panics)).
        let presence = named
            .iter()
            .map(|name| match fields.iter().find(|field| field.ident == *name) {
                Some(field) => field.was_written(),
                None => Err(Error::new_spanned(
                    name,
                    "internal: a rule named a field that resolved earlier and cannot be found \
                     now. This is a proc_macro_flow bug.",
                )),
            })
            .collect::<Result<Vec<syn::Expr>>>()?;

        // The VARIANT ident, derived from the vocabulary's own spelling rather than written out
        // again - `one_of` becomes `OneOf`, `at_most_one` becomes `AtMostOne`. A second table
        // mapping kinds to idents could disagree with the first; this cannot.
        let variant = quote::format_ident!("{}", kind.spelling().to_upper_camel_case());

        let keys = named.iter().map(|name| name.to_string());
        let violation = quote! {
            out.push(::proc_macro_flow_traits::extractor::Reason::new(
                ::proc_macro_flow_traits::extractor::ReasonKind::Violated(
                    ::proc_macro_flow_traits::assert::Violation::new(
                        ::proc_macro_flow_traits::assert::AssertKind::#variant,
                        &[ #(#keys),* ],
                    ),
                ),
            ));
        };

        // `requires` asks about ONE key's effect on another, so it is not a count at all.
        if let AssertKind::Requires = kind {
            let mut presence = presence.into_iter();
            let (first, second) = (presence.next(), presence.next());
            return parse2(quote! {
                if #first && !(#second) {
                    #violation
                }
            });
        }

        let test = match kind {
            AssertKind::OneOf => quote!(written != 1),
            AssertKind::AnyOf => quote!(written == 0),
            // `conflicts` is the same COUNT as `at_most_one` and a different thing to say about
            // it - "these do not go together" rather than "pick one". The wording is what differs,
            // and E(Violation) carries the rule so the wording can.
            AssertKind::AtMostOne | AssertKind::Conflicts => quote!(written > 1),
            AssertKind::Requires | AssertKind::With => quote!(false),
        };

        parse2(quote! {
            {
                let written = [ #(#presence),* ]
                    .into_iter()
                    .filter(|value: &bool| *value)
                    .count();
                if #test {
                    #violation
                }
            }
        })
    }
}
