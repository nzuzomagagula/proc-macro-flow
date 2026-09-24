// @review [ ]
//! What `#[derive(Processor)]` builds: the identity impl.

use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use quote::{quote, ToTokens};
use syn::{parse2, DeriveInput, ItemImpl};

use super::processor::ProcessedStage;

/// One impl, and the type says so.
pub(crate) struct ProcessorExpansion(ItemImpl);

impl ToTokens for ProcessorExpansion {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
    }
}

impl<'ast> Generator<'ast> for ProcessorExpansion {
    type Input = ProcessedStage<'ast>;
    type Subject = &'ast DeriveInput;
    type Output = Self;

    fn generate(input: ProcessedStage<'ast>) -> Extraction<Self> {
        let (impl_generics, _, where_clause) = input.generics.split_for_impl();
        let (_, type_generics, _) = input.declared.split_for_impl();
        let (name, lifetime, source) = (input.name, &input.lifetime, &input.source);

        // The Input must be what this type's own EXTRACTOR produced, and `#[args]` changes that -
        // an attribute macro's extraction is wrapped around the pair, not the bare node.
        let extracted_from = match &input.args {
            None => quote!(& #lifetime #source),
            Some(args) => quote! {
                ::proc_macro_flow_traits::attributed::Attributed<#lifetime, #args, #source>
            },
        };

        match parse2::<ItemImpl>(quote! {
            impl #impl_generics ::proc_macro_flow_traits::processor::Processor<#lifetime>
                for #name #type_generics #where_clause
            {
                type Input = ::proc_macro_flow_traits::extractor::Extracted<
                    Self,
                    #extracted_from,
                >;
                type Output = Self;

                /// Identity: the extraction IS the processed value. Children are already
                /// `Extracted` inside it, so there is nothing to cascade at this level - a
                /// processor that needs to reach them is doing real work and writes itself.
                fn process(
                    input: Self::Input,
                ) -> ::proc_macro_flow_traits::extractor::Extraction<Self::Output> {
                    input.into_extraction()
                }
            }
        }) {
            Ok(item) => Extraction::value(ProcessorExpansion(item)),
            Err(error) => Extraction::failed(Reason::new(ReasonKind::Internal(error))),
        }
    }

    /// No vacant form, for the reason ID(derive/the-impl-is-the-product) gives: without the
    /// source type there is no impl to shape, and a guessed one would compile and be wrong.
    fn stub(subject: &'ast DeriveInput) -> syn::Result<Self> {
        Err(syn::Error::new_spanned(
            &subject.ident,
            "`#[derive(Processor)]` needs `#[source(Ty)]` before it can write an impl",
        ))
    }
}
