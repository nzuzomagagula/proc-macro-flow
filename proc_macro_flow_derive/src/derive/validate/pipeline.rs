// @review [ ]
//! `#[derive(Validate)]`'s macro boundary: the three stages named so they must agree.

use proc_macro_flow_traits::pipeline::Pipeline;

use super::extractor::StageDeclaration;
use super::generator::ValidateExpansion;

// TODO[x](#processor/trivial-derives-are-pipelines): R[F(derive_validate) -> S(ValidateWiring)] && R[F(derive_processor) -> S(ProcessorWiring)],
// "The two identity derives, converted first because they prove the shape against 47 and 49 lines
// before it meets 337. They share one declaration reader and part company at the generator"
pub(crate) struct ValidateWiring;

impl<'ast> Pipeline<'ast> for ValidateWiring {
    type Extractor = StageDeclaration<'ast>;
    /// The extractor names itself, which ID(pipeline/no-processor-is-the-extractor) permits - and
    /// what it does here is real, if small: see NOTE(#stage/lifetime-is-the-processing).
    type Processor = StageDeclaration<'ast>;
    type Generator = ValidateExpansion;
}

#[cfg(test)]
mod tests {
    use super::ValidateWiring;
    use proc_macro_flow_traits::pipeline::Pipeline;
    use quote::ToTokens;
    use syn::{parse_str, DeriveInput};

    /// Drive the WHOLE pipeline, exactly as the entry function does.
    ///
    /// Not the generator alone: what is being checked is that the three stages agree and that
    /// F(run) normalises them, which is the claim the conversion makes.
    fn expand(source: &str) -> String {
        let input: DeriveInput = parse_str(source).expect("the item parses");
        ValidateWiring::run(&input).to_token_stream().to_string()
    }

    #[test]
    fn a_derive_keeps_the_bare_node_as_its_source() {
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("type Source = & 'ast DeriveInput"), "{out}");
        assert!(!out.contains("Attributed"), "a derive got the pair: {out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn declaring_args_makes_the_source_a_pair() {
        // ID(validate/source-shape-follows-args). The whole attribute-macro fix from the derive's
        // side: one extra declaration, and the Source carries both halves.
        let out = expand("#[source(ItemFn)] #[args(TraceArgs)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("Attributed < 'ast , TraceArgs , ItemFn >"), "{out}");
        // And `Valid` still narrows to the ITEM - a stage downstream wants the node, not the pair.
        assert!(out.contains("type Valid = & 'ast ItemFn"), "{out}");
    }

    #[test]
    fn a_type_with_no_lifetime_gets_one_introduced() {
        // ID(derive/lifetime-is-introduced-when-absent), and the one real decision this pipeline's
        // processor makes - NOTE(#stage/lifetime-is-the-processing).
        let out = expand("#[source(DeriveInput)] struct Read;");

        assert!(out.contains("impl < 'ast > :: proc_macro_flow_traits"), "{out}");
        // The type's own name carries NO generics, because the author declared none.
        assert!(out.contains("for Read "), "{out}");
    }

    #[test]
    fn the_source_is_asserted_visitable_at_the_authors_span() {
        // ID(declaration/assert-what-was-named). VERIFIED by probe that the assertion FIRES and
        // lands where it should: `#[source(String)]` gives
        // `the trait bound &'ast String: Visitable<'ast> is not satisfied` pointing at `String` in
        // the attribute, with the syn nodes listed as what does implement it. What this pins is
        // that the assertion is still EMITTED - the half a refactor drops.
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("Visitable"), "the source assertion is gone: {out}");
        assert!(out.contains("const _"), "{out}");
    }

    #[test]
    fn the_args_are_asserted_readable_at_the_authors_span() {
        // VERIFIED by probe: `#[args(String)]` gives `the trait bound String: FromBody is not
        // satisfied` pointing at `String`.
        let out = expand("#[source(ItemFn)] #[args(TraceArgs)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("FromBody"), "the args assertion is gone: {out}");
    }

    #[test]
    fn no_args_means_no_args_assertion() {
        // The absence is the derive case, not an omission - NOTE(#args/absence-is-the-derive-case).
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { _p: &'ast () }");

        assert!(!out.contains("FromBody"), "{out}");
    }

    #[test]
    fn the_generated_assertion_does_not_warn_in_the_authors_crate() {
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("allow (dead_code)"), "{out}");
    }

    #[test]
    fn a_missing_source_is_reported_and_no_impl_is_guessed() {
        // NOTE(#derive/the-impl-is-the-product). The source type IS the content of the impl, so
        // there is nothing to stub - a guessed one would compile and be wrong.
        let out = expand("struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("source"), "the message must name what is missing: {out}");
        assert!(!out.contains("impl < 'ast > :: proc_macro_flow_traits"), "an impl was guessed: {out}");
    }

    #[test]
    fn every_reason_reaches_the_output_not_just_the_first() {
        // ID(no-result) through the conversion: the declaration reader attempts BOTH reads
        // whatever the other did.
        let out = expand("#[args(!!)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.matches("compile_error").count() >= 2, "only one reason surfaced: {out}");
    }
}
