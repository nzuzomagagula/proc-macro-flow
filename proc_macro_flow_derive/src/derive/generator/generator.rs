// @review [ ]
//! What `#[derive(Generator)]` builds: the `Generator` impl, and the `ToTokens` that lowers it.

use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use quote::{quote, ToTokens};
use syn::{parse2, DeriveInput, ItemImpl};

use super::processor::ProcessedGenerator;

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

                fn stub(subject: Self::Subject) -> ::proc_macro_flow_traits::syn::Result<Self> {
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
             one item - and needs `#[builds(from = Ty, subject = Ty)]` beside it",
        ))
    }
}
