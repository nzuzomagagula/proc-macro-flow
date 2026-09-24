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

use proc_macro_flow_traits::assert::Assert;
use proc_macro_flow_traits::extractor::{
    Extracted, Extraction, Extractor, Reason, ReasonKind, Validate,
};
use proc_macro_flow_traits::generator::Generator;
use proc_macro_flow_traits::pipeline::Pipeline;
use proc_macro_flow_traits::processor::Processor;
use proc_macro_flow_traits::render::Diagnose;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse2, Data, DeriveInput, Error, Expr, Ident, ItemImpl, Result, Stmt, Token, Type};

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

/// A generator type, as declared.
pub(crate) struct GeneratorDeclaration<'ast> {
    name: &'ast Ident,
    declared: &'ast syn::Generics,
    wiring: Wiring,
    children: Vec<Child>,
    /// One index per field, so the `ToTokens` impl concatenates them in declaration order.
    indices: Vec<syn::Index>,
}

impl<'ast> Validate<'ast> for GeneratorDeclaration<'ast> {
    type Source = &'ast DeriveInput;
    /// A REAL narrowing: a generator IS a tuple struct, and what it wraps is what it emits.
    type Valid = &'ast syn::FieldsUnnamed;

    /// A NEWTYPE, checked rather than assumed - the `ToTokens` impl forwards to `self.0`, and
    /// ID(generation/newtype-per-item) is the whole shape this derive exists to support.
    ///
    /// N unnamed fields, not one. A multi-field tuple struct is how a generator states that it
    /// emits EXACTLY these items of EXACTLY these types - see NOTE(#pipeline/expansion-is-typed).
    fn validate(input: Self::Source) -> ::std::result::Result<Self::Valid, Reason> {
        match &input.data {
            Data::Struct(data) => match &data.fields {
                syn::Fields::Unnamed(unnamed) if !unnamed.unnamed.is_empty() => Ok(unnamed),
                _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
            },
            _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
        }
    }
}

impl<'ast> Extractor<'ast> for GeneratorDeclaration<'ast> {
    type Output = Extracted<Self, &'ast DeriveInput>;

    fn extract_from(node: &'ast DeriveInput) -> Self::Output {
        fn read(node: &DeriveInput) -> Extraction<GeneratorDeclaration<'_>> {
            let unnamed = match GeneratorDeclaration::validate(node) {
                Ok(unnamed) => unnamed,
                Err(reason) => {
                    return Extraction::failed(reason);
                }
            };

            let mut out: Extraction<GeneratorDeclaration<'_>> = Extraction::default();

            let wiring = match node.attrs.find_one("generator") {
                Ok(Some(attr)) => match attr.parse_args::<Wiring>() {
                    Ok(wiring) => Some(wiring),
                    Err(error) => {
                        out.reasons.push(Reason::new(ReasonKind::Syntax(error)));
                        None
                    }
                },
                Ok(None) => {
                    out.reasons.push(Reason::new(ReasonKind::Syntax(syn::Error::new_spanned(
                        &node.ident,
                        "`#[generator(from = Ty, subject = Ty)]` says what this consumes and what \
                         a stub is written against",
                    ))));
                    None
                }
                Err(error) => {
                    out.reasons.push(Reason::new(ReasonKind::Syntax(error)));
                    None
                }
            };

            // Attempted whatever the wiring did - ID(no-result).
            let children = match node.attrs.find_one("generates") {
                Ok(None) => Some(Vec::new()),
                Ok(Some(attr)) => {
                    match attr.parse_args_with(Punctuated::<Child, Token![,]>::parse_terminated) {
                        Ok(written) => Some(written.into_iter().collect()),
                        Err(error) => {
                            out.reasons.push(Reason::new(ReasonKind::Syntax(error)));
                            None
                        }
                    }
                }
                Err(error) => {
                    out.reasons.push(Reason::new(ReasonKind::Syntax(error)));
                    None
                }
            };

            if let (Some(wiring), Some(children)) = (wiring, children) {
                out.value = Some(GeneratorDeclaration {
                    name: &node.ident,
                    declared: &node.generics,
                    wiring,
                    children,
                    indices: (0..unnamed.unnamed.len()).map(syn::Index::from).collect(),
                });
            }

            out
        }

        Extracted::new(read(node), node)
    }
}

impl Assert for GeneratorDeclaration<'_> {}

impl Diagnose for GeneratorDeclaration<'_> {
    /// A leaf: its children are DECLARATIONS carried verbatim, not extractions of their own.
    fn diagnose(&self, _: &mut Vec<syn::Error>) {}
}

/// Every declared child turned into the statements that produce it.
pub(crate) struct ProcessedGenerator<'ast> {
    name: &'ast Ident,
    declared: &'ast syn::Generics,
    generics: syn::Generics,
    lifetime: syn::Lifetime,
    from: Type,
    subject: Type,
    names: Vec<Ident>,
    calls: Vec<Stmt>,
    stubs: Vec<Stmt>,
    indices: Vec<syn::Index>,
}

impl<'ast> Processor<'ast> for GeneratorDeclaration<'ast> {
    type Input = Extracted<GeneratorDeclaration<'ast>, &'ast DeriveInput>;
    type Output = ProcessedGenerator<'ast>;

    /// The real work: each declared child becomes a statement that calls it, absorbs its result
    /// and isolates its failure, and a second that produces its vacant form.
    fn process(input: Self::Input) -> Extraction<Self::Output> {
        let node = *input.source();
        // NOTE(#processor/reasons-are-new-not-inherited).
        let mut out: Extraction<Self::Output> = Extraction::default();

        let Some(value) = input.into_extraction().value else {
            return out;
        };

        // A leaf wrapping a syn item borrows nothing, so it may have no lifetime of its own - see
        // NOTE(#derive/lifetime-is-introduced-when-absent).
        let (generics, lifetime) = super::stage_lifetime(node);

        let mut names = Vec::new();
        let mut calls = Vec::new();
        let mut stubs = Vec::new();
        for child in &value.children {
            match (child.call(), child.stub()) {
                (Ok(call), Ok(stub)) => {
                    names.push(child.name.clone());
                    calls.push(call);
                    stubs.push(stub);
                }
                (Err(error), _) | (_, Err(error)) => {
                    out.reasons.push(Reason::new(ReasonKind::Internal(error)));
                }
            }
        }

        out.value = Some(ProcessedGenerator {
            name: value.name,
            declared: value.declared,
            generics,
            lifetime,
            from: value.wiring.from,
            subject: value.wiring.subject,
            names,
            calls,
            stubs,
            indices: value.indices,
        });
        out
    }
}

/// The `Generator` impl and the `ToTokens` that lowers what it built.
pub(crate) struct GeneratorExpansion(ItemImpl, ItemImpl);

impl ToTokens for GeneratorExpansion {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
        self.1.to_tokens(tokens);
    }
}

impl<'ast> Generator<'ast> for GeneratorExpansion {
    type Input = ProcessedGenerator<'ast>;
    type Subject = &'ast DeriveInput;
    type Output = Self;

    fn generate(input: ProcessedGenerator<'ast>) -> Extraction<Self> {
        let (impl_generics, _, _) = input.generics.split_for_impl();
        let (to_tokens_generics, type_generics, where_clause) = input.declared.split_for_impl();
        let (name, lifetime) = (input.name, &input.lifetime);
        let (from, subject) = (&input.from, &input.subject);
        let (names, calls, stubs) = (&input.names, &input.calls, &input.stubs);
        let indices = &input.indices;

        let generator = parse2::<ItemImpl>(quote! {
            impl #impl_generics ::proc_macro_flow_traits::generator::Generator<#lifetime>
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
                            out.reasons.push(
                                ::proc_macro_flow_traits::extractor::Reason::new(
                                    ::proc_macro_flow_traits::extractor::ReasonKind::Internal(
                                        error,
                                    ),
                                ),
                            );
                            out
                        }
                    }
                }

                fn stub(subject: Self::Subject) -> ::syn::Result<Self> {
                    #(#stubs)*
                    Self::assemble_stub(subject, #(#names),*)
                }
            }
        });

        // Forwarding boilerplate. Ty(Output) is bound `ToTokens`, and for a newtype that impl is
        // always the same one line - so the author writing it by hand would be the derive failing
        // to do its job. Every field, in declaration order: the item they compose to is their
        // concatenation.
        let to_tokens = parse2::<ItemImpl>(quote! {
            impl #to_tokens_generics ::proc_macro_flow_traits::quote::ToTokens
                for #name #type_generics #where_clause
            {
                fn to_tokens(
                    &self,
                    tokens: &mut ::proc_macro_flow_traits::proc_macro2::TokenStream,
                ) {
                    #(::proc_macro_flow_traits::quote::ToTokens::to_tokens(
                        &self.#indices,
                        tokens,
                    );)*
                }
            }
        });

        match (generator, to_tokens) {
            (Ok(generator), Ok(to_tokens)) => {
                Extraction::value(GeneratorExpansion(generator, to_tokens))
            }
            (Err(error), _) | (_, Err(error)) => {
                Extraction::failed(Reason::new(ReasonKind::Internal(error)))
            }
        }
    }

    /// No vacant form - NOTE(#derive/the-impl-is-the-product).
    fn stub(subject: &'ast DeriveInput) -> syn::Result<Self> {
        Err(syn::Error::new_spanned(
            &subject.ident,
            "`#[derive(Generator)]` describes a NEWTYPE wrapping the item it generates - \
             `struct Fields(syn::ImplItem);` - or several, for a generator that emits more than \
             one item - and needs `#[generator(from = Ty, subject = Ty)]` beside it",
        ))
    }
}

/// `#[derive(Generator)]`, wired.
// TODO[ ](#generator/derive-is-a-pipeline): R[F(derive_generator) -> S(GeneratorWiring)], "Its
// validate is the newtype check - a generator IS a tuple struct, and what it wraps is what it
// emits (ID(generation/newtype-per-item))"
pub(crate) struct GeneratorWiring;

impl<'ast> Pipeline<'ast> for GeneratorWiring {
    type Extractor = GeneratorDeclaration<'ast>;
    type Processor = GeneratorDeclaration<'ast>;
    type Generator = GeneratorExpansion;
}

impl Child {
    /// The call that produces this child, with per-child failure isolation.
    /// The statement that produces this child, with per-child failure isolation.
    fn call(&self) -> Result<Stmt> {
        let name = &self.name;
        let leaf = self.leaf();
        let feed = &self.feed;

        let tokens = match self.declared.arity() {
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
        };

        // A STATEMENT, parsed here rather than handed on as loose tokens - nothing malformed
        // leaves this function. See NOTE(#pipeline/expansion-is-typed).
        parse2(tokens)
    }

    /// The same child, vacant.
    /// The same child, vacant.
    fn stub(&self) -> Result<Stmt> {
        let name = &self.name;
        let leaf = self.leaf();

        let tokens = match self.declared.arity() {
            // A repeated child with nothing written is NONE of them, not one empty one.
            Arity::Many => quote! {
                let #name: ::std::vec::Vec<#leaf> = ::std::vec::Vec::new();
            },
            _ => quote! {
                let #name = <#leaf as ::proc_macro_flow_traits::generator::Generator>
                    ::stub(::std::default::Default::default())?;
            },
        };

        // A STATEMENT, parsed here rather than handed on as loose tokens - nothing malformed
        // leaves this function. See NOTE(#pipeline/expansion-is-typed).
        parse2(tokens)
    }
}
