// @review [ ]
//! `#[derive(Generator)]` — a parent declares its children and what it feeds each one.
//!
//! NOTE(#generator-derive/plumbing-not-logic): V[F(derive_generator).emits(composition) && !emits(assemble)],
//! "The derive emits the COMPOSITION - call each child with its fed input, absorb the result, stub
//! the ones that failed - and requires the author to write F(assemble), which puts the children
//! into an item. That split is forced rather than chosen: the derive cannot know what SHAPE a
//! parent's item is. It knows there are two children; it cannot know they belong inside
//! `impl Thing { .. }` rather than a module or a match arm.
//!
//! Which is the same split ID(processor/derive-enforces) settles for processing, and it lands the
//! same way: a missing F(assemble) is a compile error, not a silently trivial generator"

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse2, Data, DeriveInput, Error, Expr, Ident, Item, ItemImpl, Result, Token, Type};

use super::Arity;
use super::ext::{AttributesExt, TypeExt};

/// One declared child: its name, its type, and what the parent feeds it.
struct Child {
    name: Ident,
    /// The child's type as WRITTEN, so arity is read off it - ID(from/arity-from-type).
    declared: Type,
    /// Spliced verbatim and never inspected, the Attr(from) bargain.
    feed: Expr,
}

impl Parse for Child {
    fn parse(input: ParseStream) -> Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let declared = input.parse()?;
        input.parse::<Token![=]>()?;
        let feed = input.parse()?;
        Ok(Child {
            name,
            declared,
            feed,
        })
    }
}

impl Child {
    /// The child's own type, with any `Vec<..>` peeled off.
    fn leaf(&self) -> &Type {
        self.declared.inner()
    }
}

/// `from = Ty, subject = Ty` — what this generator consumes, and what a stub is written against.
struct Wiring {
    from: Type,
    subject: Type,
}

impl Parse for Wiring {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut from = None;
        let mut subject = None;

        for pair in Punctuated::<Assign, Token![,]>::parse_terminated(input)? {
            match pair.key.to_string().as_str() {
                "from" => from = Some(pair.value),
                "subject" => subject = Some(pair.value),
                other => {
                    return Err(Error::new_spanned(
                        &pair.key,
                        format!("`{other}` is not one of `from`, `subject`"),
                    ));
                }
            }
        }

        Ok(Wiring {
            from: from.ok_or_else(|| input.error("`from = Ty` names what this generator consumes"))?,
            subject: subject
                .ok_or_else(|| input.error("`subject = Ty` names what a stub is written against"))?,
        })
    }
}

struct Assign {
    key: Ident,
    value: Type,
}

impl Parse for Assign {
    fn parse(input: ParseStream) -> Result<Self> {
        let key = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(Assign {
            key,
            value: input.parse()?,
        })
    }
}

pub(crate) fn derive_generator(input: DeriveInput) -> Result<Vec<Item>> {
    let name = &input.ident;

    // A NEWTYPE, checked rather than assumed - the ToTokens impl below forwards to `self.0`, and
    // ID(generation/newtype-per-item) is the whole shape this derive exists to support.
    match &input.data {
        Data::Struct(data) if data.fields.len() == 1 && data.fields.iter().all(|f| f.ident.is_none()) => {}
        _ => {
            return Err(Error::new_spanned(
                name,
                "`#[derive(Generator)]` describes a NEWTYPE wrapping the item it generates - \
                 `struct Fields(syn::ImplItem);`",
            ));
        }
    }

    let wiring: Wiring = input
        .attrs
        .find_one("generator")?
        .ok_or_else(|| {
            Error::new_spanned(
                name,
                "`#[generator(from = Ty, subject = Ty)]` says what this consumes and what a stub \
                 is written against",
            )
        })?
        .parse_args()?;

    let children: Vec<Child> = match input.attrs.find_one("generates")? {
        None => Vec::new(),
        Some(attr) => attr
            .parse_args_with(Punctuated::<Child, Token![,]>::parse_terminated)?
            .into_iter()
            .collect(),
    };

    let (from, subject) = (&wiring.from, &wiring.subject);
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

    let calls: Vec<TokenStream> = children.iter().map(Child::call).collect();
    let names: Vec<&Ident> = children.iter().map(|child| &child.name).collect();
    let stubs: Vec<TokenStream> = children.iter().map(Child::stub).collect();

    let generator = parse2::<ItemImpl>(quote! {
        impl #impl_generics ::proc_macro_flow_traits::generator::Generator
            for #name #type_generics #where_clause
        {
            type Input = #from;
            type Subject = #subject;
            type Output = Self;

            fn generate(
                input: Self::Input,
            ) -> ::proc_macro_flow_traits::extractor::Extraction<Self> {
                let mut out = ::proc_macro_flow_traits::extractor::Extraction::default();

                #(#calls)*

                // The author supplies the SHAPE; the derive supplied everything else. See
                // NOTE(#generator-derive/plumbing-not-logic).
                match Self::assemble(&input, #(#names),*) {
                    ::std::result::Result::Ok(item) => {
                        out.value = ::std::option::Option::Some(item);
                        out
                    }
                    ::std::result::Result::Err(error) => {
                        out.reasons.push(::proc_macro_flow_traits::extractor::Reason::new(
                            ::proc_macro_flow_traits::extractor::ReasonKind::Internal(error),
                        ));
                        out
                    }
                }
            }

            fn stub(subject: Self::Subject) -> ::syn::Result<Self> {
                #(#stubs)*
                Self::assemble_stub(subject, #(#names),*)
            }
        }
    })?;

    // Forwarding boilerplate. Ty(Output) is bound `ToTokens`, and for a newtype that impl is
    // always the same one line - so the author writing it by hand would be the derive failing to
    // do its job.
    let to_tokens = parse2::<ItemImpl>(quote! {
        impl #impl_generics ::proc_macro_flow_traits::quote::ToTokens for #name #type_generics #where_clause {
            fn to_tokens(&self, tokens: &mut ::proc_macro_flow_traits::proc_macro2::TokenStream) {
                ::proc_macro_flow_traits::quote::ToTokens::to_tokens(&self.0, tokens);
            }
        }
    })?;

    Ok(vec![Item::Impl(generator), Item::Impl(to_tokens)])
}

impl Child {
    /// The call that produces this child, with per-child failure isolation.
    fn call(&self) -> TokenStream {
        let name = &self.name;
        let leaf = self.leaf();
        let feed = &self.feed;

        match self.declared.arity() {
            // MANY: one call per element. A failure isolates to the ELEMENT, not the whole child,
            // which is what makes ID(generation/parent-feeds-children) worth the extra syntax.
            Arity::Many => quote! {
                let #name: ::std::vec::Vec<#leaf> = (#feed)
                    .into_iter()
                    .filter_map(|fed| {
                        // FAIL UPWARD: a reason that is not OURS adopts the span of the source
                        // this element came from, so the author sees the syntax responsible.
                        let span = ::syn::spanned::Spanned::span(&fed);
                        let produced = <#leaf as ::proc_macro_flow_traits::generator::Generator>
                            ::generate(fed);
                        let value = out.absorb(::proc_macro_flow_traits::extractor::Extraction {
                            value: produced.value,
                            reasons: produced
                                .reasons
                                .into_iter()
                                .map(|reason| if reason.is_internal() {
                                    reason
                                } else {
                                    reason.or_span(span)
                                })
                                .collect(),
                        });
                        value
                    })
                    .collect();
            },
            // ONE: stub on failure so the siblings still reach the output.
            _ => quote! {
                let #name = match out.absorb(
                    <#leaf as ::proc_macro_flow_traits::generator::Generator>::generate(#feed),
                ) {
                    ::std::option::Option::Some(item) => item,
                    ::std::option::Option::None => {
                        match <#leaf as ::proc_macro_flow_traits::generator::Generator>
                            ::stub(::std::default::Default::default())
                        {
                            ::std::result::Result::Ok(vacant) => vacant,
                            ::std::result::Result::Err(error) => {
                                out.reasons.push(
                                    ::proc_macro_flow_traits::extractor::Reason::new(
                                        ::proc_macro_flow_traits::extractor::ReasonKind::Internal(
                                            error,
                                        ),
                                    ),
                                );
                                return out;
                            }
                        }
                    }
                };
            },
        }
    }

    /// The same child, vacant.
    fn stub(&self) -> TokenStream {
        let name = &self.name;
        let leaf = self.leaf();

        match self.declared.arity() {
            // A repeated child with nothing written is NONE of them, not one empty one.
            Arity::Many => quote! {
                let #name: ::std::vec::Vec<#leaf> = ::std::vec::Vec::new();
            },
            _ => quote! {
                let #name = <#leaf as ::proc_macro_flow_traits::generator::Generator>
                    ::stub(::std::default::Default::default())?;
            },
        }
    }
}
