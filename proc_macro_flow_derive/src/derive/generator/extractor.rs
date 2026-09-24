// @review [ ]
//! Reading a generator's declaration: what it consumes, and which children it composes.

use proc_macro_flow_traits::assert::Assert;
use proc_macro_flow_traits::extractor::{
    Extracted, Extraction, Extractor, Reason, ReasonKind, Validate,
};
use proc_macro_flow_traits::render::Diagnose;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse2, Data, DeriveInput, Error, Expr, Ident, Result, Stmt, Token, Type};

use super::super::Arity;
use super::super::ext::{AttributesExt, TypeExt};

/// One declared child: its name, its type, and what the parent feeds it.
pub(crate) struct Child {
    pub(crate) name: Ident,
    /// The child's type as WRITTEN, so arity is read off it - ID(from/arity-from-type).
    pub(crate) declared: Type,
    /// Spliced verbatim and never inspected, the Attr(from) bargain.
    pub(crate) feed: Expr,
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
pub(crate) struct Wiring {
    pub(crate) from: Type,
    pub(crate) subject: Type,
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
    pub(crate) key: Ident,
    pub(crate) value: Type,
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
    pub(crate) name: &'ast Ident,
    pub(crate) declared: &'ast syn::Generics,
    pub(crate) wiring: Wiring,
    pub(crate) children: Vec<Child>,
    /// One index per field, so the `ToTokens` impl concatenates them in declaration order.
    pub(crate) indices: Vec<syn::Index>,
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

impl Child {
    /// The call that produces this child, with per-child failure isolation.
    /// The statement that produces this child, with per-child failure isolation.
    pub(crate) fn call(&self) -> Result<Stmt> {
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
    pub(crate) fn stub(&self) -> Result<Stmt> {
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
