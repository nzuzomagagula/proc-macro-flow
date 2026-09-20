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

use syn::{parse_quote, DeriveInput, ItemImpl};

use crate::base::extractor::processor::ProcessedStruct;
use crate::base::syntax::extractor::SyntaxHelper;

// TODO(#generator/macro):C[F(generator)], "Proc-macro entry point for the generator stage, alongside lib.rs::field_names (ID(extractor/macro-wiring))"
// TODO[x](#typed-output/generate):U[F(generate).R(TokenStream) -> R(syn::ItemImpl)], "DONE. Both
// functions return ItemImpl, built with parse_quote!, so nothing malformed leaves this file -
// a mistake is a panic HERE rather than a mystery rustc error in the author's crate. See
// NOTE(#generator/parse-quote-panics) for why panicking is the right signal for this specific
// failure. ID(typed-output/spans) now has a specific item to point at and stays open"
// NOTE(#generator/shapes-is-a-probe): V[C(SHAPES).proves(cascade)], "SHAPES is not a feature. It
// exists to demonstrate that a THIRD-LEVEL value - an attribute's carried tokens - reaches
// generation, which was false until ProcessedField stopped dropping its attrs. It is the assertion
// that would have failed before the cascade, written as generated code. Decide the real product
// with the finished pipeline in view, and delete this if it is not part of it"

impl<'ast> proc_macro_flow_traits::generator::Generator for ProcessedStruct<'ast> {
    type Input = Self;

    /// The node this output is written ABOUT - see the naming rule on Tr(Generator)::Subject.
    /// Matches `Validate::Source` exactly so Tr(Pipeline) can bind the two.
    type Subject = &'ast DeriveInput;

    /// The most specific type that fits: this generator emits exactly one impl block.
    ///
    /// NOT `syn::Item`. Naming the narrowest level is the point of
    /// NOTE(#typed-output/level-is-associated) - it is what lets a future parent embed this
    /// generator's output without either side lowering to tokens.
    type Output = ItemImpl;

    fn generate(input: Self) -> ItemImpl {
        let name = &input.item.ident;
        let (impl_generics, type_generics, where_clause) = input.item.generics.split_for_impl();

        // Unnamed fields are addressed positionally, which is what a tuple struct's "name" is.
        let names = input.fields.iter().enumerate().map(|(index, processed)| {
            match &processed.field.ident {
                Some(ident) => ident.to_string(),
                None => index.to_string(),
            }
        });

        // The THIRD level, and the only reason this const exists. A field's grammar attributes are
        // extracted, processed and now readable here - which was not true until the cascade
        // landed, because ProcessedField dropped them. See ID(generator/shapes-is-a-probe).
        let shapes = input.fields.iter().map(|processed| {
            processed
                .attrs
                .iter()
                .find(|attribute| attribute.helper == SyntaxHelper::Shape)
                .map(|attribute| attribute.tokens.to_string())
                .unwrap_or_default()
        });

        parse_quote! {
            impl #impl_generics #name #type_generics #where_clause {
                pub const FIELDS: &'static [&'static str] = &[ #(#names),* ];
                pub const SHAPES: &'static [&'static str] = &[ #(#shapes),* ];
            }
        }
    }

    fn stub(subject: &'ast DeriveInput) -> ItemImpl {
        let name = &subject.ident;
        let (impl_generics, type_generics, where_clause) = subject.generics.split_for_impl();

        parse_quote! {
            impl #impl_generics #name #type_generics #where_clause {
                pub const FIELDS: &'static [&'static str] = &[];
                pub const SHAPES: &'static [&'static str] = &[];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro_flow_traits::pipeline::Pipeline;
    use syn::parse_str;

    use crate::base::extractor::pipeline::ExtractorPipeline;

    /// The whole pipeline, exactly as `lib.rs::field_names` runs it.
    ///
    /// One call now. This helper used to re-assemble the stages by hand - extract, render,
    /// process, emit - which meant the test could drift from the entry point without either
    /// noticing. NOTE(#pipeline/owns-normalisation) removed the opportunity.
    fn pipeline(source: &str) -> String {
        let item: &'static DeriveInput =
            Box::leak(Box::new(parse_str(source).expect("the item parses")));

        ExtractorPipeline::run(item).to_string()
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
        assert!(
            out.contains("compile_error"),
            "the reason is missing: {out}"
        );
    }

    #[test]
    fn the_stub_precedes_the_error() {
        let out = pipeline("pub enum Thing { A, B }");
        assert!(
            out.find("impl").unwrap() < out.find("compile_error").unwrap(),
            "{out}"
        );
    }

    #[test]
    fn a_shape_attribute_reaches_generation() {
        // THE assertion the cascade exists for, and the one that would have failed before it:
        // an attribute's carried tokens are a THIRD-LEVEL value, and ProcessedField used to drop
        // them on the way to here. See NOTE(#generator/shapes-is-a-probe).
        let out = pipeline("pub struct Thing { #[shape(AttributeKind::MetaList)] a: u8 }");

        assert!(out.contains("SHAPES"), "{out}");
        assert!(out.contains("AttributeKind :: MetaList"), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn a_field_with_no_grammar_attribute_gets_an_empty_shape() {
        let out = pipeline("pub struct Thing { a: u8 }");

        assert!(out.contains("SHAPES"), "{out}");
        assert!(!out.contains("AttributeKind"), "{out}");
    }

    #[test]
    fn a_doc_comment_does_not_become_a_shape_or_an_error() {
        // ID(heads-are-rustcs), now checked through the WHOLE pipeline rather than at the
        // extraction tree - the processor touching attributes must not change this.
        let out = pipeline("pub struct Thing { /// documented\n a: u8 }");

        assert!(!out.contains("compile_error"), "{out}");
        assert!(!out.contains("doc"), "{out}");
    }
}
