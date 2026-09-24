// @review [ ]
//! `#[derive(Processor)]` — the identity, as a pipeline.
//!
//! Most extractions copy their fields through and generate from them with no transformation, so
//! the default has to cost nothing. Deriving this IS the opt-in: a type needing real processing
//! omits the derive and writes `impl Processor` by hand, which is ordinary Rust and needs no
//! opt-out attribute.
//!
//! It shares its extractor and processor with `#[derive(Validate)]` and differs only here, at the
//! generator - see NOTE(#stage/one-reader-two-generators).

use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use proc_macro_flow_traits::pipeline::Pipeline;
use quote::{quote, ToTokens};
use syn::{parse2, DeriveInput, ItemImpl};

use super::stage::{ProcessedStage, StageDeclaration};

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

    /// No vacant form, for the reason NOTE(#derive/the-impl-is-the-product) gives: without the
    /// source type there is no impl to shape, and a guessed one would compile and be wrong.
    fn stub(subject: &'ast DeriveInput) -> syn::Result<Self> {
        Err(syn::Error::new_spanned(
            &subject.ident,
            "`#[derive(Processor)]` needs `#[source(Ty)]` before it can write an impl",
        ))
    }
}

/// `#[derive(Processor)]`, wired.
pub(crate) struct ProcessorWiring;

impl<'ast> Pipeline<'ast> for ProcessorWiring {
    type Extractor = StageDeclaration<'ast>;
    type Processor = StageDeclaration<'ast>;
    type Generator = ProcessorExpansion;
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    fn expand(source: &str) -> String {
        let input: DeriveInput = parse_str(source).expect("the item parses");
        ProcessorWiring::run(&input).to_token_stream().to_string()
    }

    #[test]
    fn the_identity_takes_its_own_extraction_and_hands_it_back() {
        let out = expand("#[source(Field)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("Extracted < Self , & 'ast Field ,"), "{out}");
        assert!(out.contains("type Output = Self"), "{out}");
        assert!(out.contains("into_extraction"), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn an_attribute_stages_input_is_wrapped_around_the_pair() {
        // The Input must match what this type's own extractor PRODUCED, and for an attribute macro
        // that is an extraction of the pair. Getting this wrong is a mismatch at the Pipeline
        // bound rather than anything subtle, but only if it is written at all.
        let out = expand("#[source(ItemFn)] #[args(TraceArgs)] struct Read<'ast> { _p: &'ast () }");

        assert!(
            out.contains("Attributed < 'ast , TraceArgs , ItemFn >"),
            "{out}"
        );
    }

    #[test]
    fn a_missing_source_is_reported_and_no_impl_is_guessed() {
        let out = expand("struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("compile_error"), "{out}");
        assert!(!out.contains("impl < 'ast >"), "an impl was guessed: {out}");
    }
}
