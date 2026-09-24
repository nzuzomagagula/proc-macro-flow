// @review [ ]
//! `#[derive(Processor)]`'s macro boundary.

use proc_macro_flow_traits::pipeline::Pipeline;

use super::extractor::StageDeclaration;
use super::generator::ProcessorExpansion;

/// `#[derive(Processor)]`, wired.
pub(crate) struct ProcessorWiring;

impl<'ast> Pipeline<'ast> for ProcessorWiring {
    type Extractor = StageDeclaration<'ast>;
    type Processor = StageDeclaration<'ast>;
    type Generator = ProcessorExpansion;
}

#[cfg(test)]
mod tests {
    use super::ProcessorWiring;
    use proc_macro_flow_traits::pipeline::Pipeline;
    use quote::ToTokens;
    use syn::{parse_str, DeriveInput};

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
