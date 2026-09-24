// @review [ ]
//! `#[derive(Extractor)]`'s macro boundary.

use proc_macro_flow_traits::pipeline::Pipeline;

use super::extractor::ExtractorExtraction;
use super::generator::ExtractorExpansion;

/// `#[derive(Extractor)]`, wired.
// TODO[x](#extractor/derive-is-a-pipeline): R[F(derive_extractor) -> S(ExtractorWiring)], "The
// first with a REAL validate - a DeriveInput narrowed to named fields - and a real processor:
// arity off each field's written type picks F(extract_from), F(extract_each) or F(extract_maybe)"
pub(crate) struct ExtractorWiring;

impl<'ast> Pipeline<'ast> for ExtractorWiring {
    type Extractor = ExtractorExtraction<'ast>;
    type Processor = ExtractorExtraction<'ast>;
    type Generator = ExtractorExpansion;
}

#[cfg(test)]
mod tests {
    use super::ExtractorWiring;
    use proc_macro_flow_traits::pipeline::Pipeline;
    use quote::ToTokens;
    use syn::{parse_str, DeriveInput};

    fn expand(source: &str) -> String {
        let input: DeriveInput = parse_str(source).expect("the item parses");
        ExtractorWiring::run(&input).to_token_stream().to_string()
    }

    const ONE_CHILD: &str = "#[source(DeriveInput)] struct Read<'ast> { \
         #[from(source.attrs.iter())] attrs: Vec<Extracted<Child<'ast>, &'ast Attribute>> }";

    #[test]
    fn three_impls_come_out_and_the_type_says_three() {
        let out = expand(ONE_CHILD);

        assert!(out.contains("Extractor < 'ast > for Read"), "{out}");
        assert!(out.contains("Diagnose for Read"), "{out}");
        assert!(out.contains("Assert for Read"), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn arity_off_the_type_picks_the_method() {
        // ID(extractor-derive/arity-picks-the-method). Nothing beside the field declares this.
        assert!(expand(ONE_CHILD).contains("extract_each"), "Vec should cascade");

        let one = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             #[from(source.ident)] it: Extracted<Child<'ast>, &'ast Ident> }",
        );
        assert!(one.contains("extract_from ("), "{one}");

        let maybe = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             #[from(source.first())] it: Option<Extracted<Child<'ast>, &'ast Ident>> }",
        );
        assert!(maybe.contains("extract_maybe"), "{maybe}");
    }

    #[test]
    fn with_applies_its_callable_to_the_validated_source() {
        let out = expand(
            "#[source(Attribute)] struct Read<'ast> { \
             #[with(|a: &'ast Attribute| ::core::iter::once(a))] \
             own: Vec<Extracted<Child<'ast>, &'ast Attribute>> }",
        );

        assert!(out.contains(") (source)"), "{out}");
    }

    #[test]
    fn diagnose_visits_every_declared_child() {
        // A forgotten visit is a silently unreachable subtree, which is why this is generated
        // rather than opt-in - NOTE(#derive/diagnose-rides-along).
        let out = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             #[from(a())] a: Vec<Extracted<C<'ast>, &'ast I>>, \
             #[from(b())] b: Vec<Extracted<C<'ast>, &'ast I>> }",
        );

        assert!(out.contains("diagnose (& self . a"), "{out}");
        assert!(out.contains("diagnose (& self . b"), "{out}");
    }

    #[test]
    fn a_field_declaring_both_ways_to_reach_it_is_reported() {
        let out = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             #[from(a())] #[with(b)] a: Vec<Extracted<C<'ast>, &'ast I>> }",
        );

        assert!(out.contains("compile_error"), "{out}");
    }

    #[test]
    fn a_field_declaring_neither_is_reported() {
        let out = expand(
            "#[source(DeriveInput)] struct Read<'ast> { a: Vec<Extracted<C<'ast>, &'ast I>> }",
        );

        assert!(out.contains("compile_error"), "{out}");
    }

    #[test]
    fn every_bad_field_is_reported_not_just_the_first() {
        // ID(no-result) through the conversion: the processor carries on past a field it could
        // not read, so a struct with two mistakes reports two.
        let out = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             a: Vec<Extracted<C<'ast>, &'ast I>>, \
             b: Vec<Extracted<C<'ast>, &'ast I>> }",
        );

        assert_eq!(out.matches("compile_error").count(), 2, "{out}");
    }

    #[test]
    fn an_enum_is_not_an_extraction_type() {
        // A REAL validate, which is what distinguishes this derive from the two trivial ones.
        let out = expand("#[source(DeriveInput)] enum Read<'ast> { A(&'ast ()) }");

        assert!(out.contains("compile_error"), "{out}");
        assert!(!out.contains("impl < 'ast > :: proc_macro_flow_traits :: extractor"), "{out}");
    }

    #[test]
    fn a_field_that_does_not_hold_an_extraction_is_reported_against_its_type() {
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { #[from(x())] a: &'ast str }");

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("Extracted"), "the message must name the shape wanted: {out}");
    }
}
