// @review [ ]
//! The concrete generator for the extractor stage.
//!
//! Answer(#generator/base-scope):A[ID(generator/base-scope) ==? F(stub)], "The question was whether
//! a base generator emits just the ItemImpl shape or the full body. Answered by building one: it
//! emits the FULL body when there is a value, and a STUB with the same shape when there is not.
//! Both are the same impl block from a use site's point of view, which is the whole requirement -
//! ID(generator/stub-alongside-errors) needs the item to exist, not to be complete"
//!
//! NOTE(#generator/stub-is-not-empty): V[F(stub).emits(Impl)], "The stub is an impl with the same
//! associated items as a successful one, just vacant - NOT an empty token stream. An empty one
//! would leave every use site reporting 'no associated item named FIELDS', which is the cascade the
//! rule exists to prevent. A stub that type-checks buys silence downstream so the real diagnostic
//! is the only thing the user reads"

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use crate::base::extractor::processor::ProcessedStruct;

// TODO(#generator/macro):C[F(generator)], "Proc-macro entry point for the generator stage, alongside lib.rs::extractor"
// TODO[ ](#typed-output/generate):U[F(generate).R(TokenStream) -> R(syn::ItemImpl)], "Both functions
// here build an impl block and hand it back as a raw stream, which is exactly the case
// @group(#typed-output) in proc_macro_flow_traits::generator names: we KNOW the shape, so returning
// ItemImpl would let nothing malformed leave this file and would give ID(typed-output/spans) a
// specific item to point at"

impl<'ast> proc_macro_flow_traits::generator::Generator for ProcessedStruct<'ast> {
    type Input = Self;

    fn generate(input: Self) -> TokenStream {
        let name = &input.item.ident;
        let (impl_generics, type_generics, where_clause) = input.item.generics.split_for_impl();

        // Unnamed fields are addressed positionally, which is what a tuple struct's "name" is.
        let names = input.fields.iter().enumerate().map(|(index, processed)| {
            match &processed.field.ident {
                Some(ident) => ident.to_string(),
                None => index.to_string(),
            }
        });

        quote! {
            impl #impl_generics #name #type_generics #where_clause {
                pub const FIELDS: &'static [&'static str] = &[ #(#names),* ];
            }
        }
    }
}

impl<'ast> ProcessedStruct<'ast> {
    /// The same impl, vacant, for when extraction produced nothing.
    ///
    /// Takes the `DeriveInput` rather than a `ProcessedStruct` because there is no processed value
    /// in the case this exists for.
    pub(crate) fn stub(item: &DeriveInput) -> TokenStream {
        let name = &item.ident;
        let (impl_generics, type_generics, where_clause) = item.generics.split_for_impl();

        quote! {
            impl #impl_generics #name #type_generics #where_clause {
                pub const FIELDS: &'static [&'static str] = &[];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro_flow_traits::{
        extractor::Extractor, generator::Generator, processor::Processor,
    };
    use syn::parse_str;

    use crate::base::extractor::extractor::StructExtraction;

    /// The whole pipeline, as `lib.rs` runs it.
    fn pipeline(source: &str) -> String {
        let item: &'static DeriveInput =
            Box::leak(Box::new(parse_str(source).expect("the item parses")));

        let processed = StructExtraction::process(StructExtraction::extract_from(item));

        let body = match processed.value {
            Some(value) => ProcessedStruct::generate(value),
            None => ProcessedStruct::stub(item),
        };

        proc_macro_flow_traits::generator::emit(
            body,
            &processed.reasons,
            item,
            "could not extract",
        )
        .to_string()
    }

    #[test]
    fn a_struct_generates_its_field_names() {
        let out = pipeline("pub struct Thing { a: u8, b: String }");

        assert!(out.contains("impl Thing"), "{out}");
        assert!(out.contains(r#""a""#), "{out}");
        assert!(out.contains(r#""b""#), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn a_tuple_struct_names_its_fields_positionally() {
        let out = pipeline("pub struct Thing(u8, String);");
        assert!(out.contains(r#""0""#), "{out}");
        assert!(out.contains(r#""1""#), "{out}");
    }

    #[test]
    fn generics_are_carried_through() {
        let out = pipeline("pub struct Thing<T: Clone> { a: T }");
        assert!(out.contains("impl < T : Clone > Thing < T >"), "{out}");
    }

    #[test]
    fn a_failed_extraction_still_emits_the_impl() {
        // THE rule. An enum is rejected, so there is no value - and the impl must exist anyway,
        // or every use site reports 'no associated item named FIELDS' on top of the real error.
        let out = pipeline("pub enum Thing { A, B }");

        assert!(out.contains("impl Thing"), "the stub is missing: {out}");
        assert!(out.contains("FIELDS"), "the stub is vacant of items: {out}");
        assert!(out.contains("compile_error"), "the reason is missing: {out}");
    }

    #[test]
    fn the_stub_precedes_the_error() {
        let out = pipeline("pub enum Thing { A, B }");
        assert!(
            out.find("impl").unwrap() < out.find("compile_error").unwrap(),
            "{out}"
        );
    }
}
