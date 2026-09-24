// @review [ ]
//! The pipeline macro: an extractor, a processor and a generator, like everything else.
//!
//! NOTE(#pipeline-macro/no-bespoke-orchestration): V[F(expand).body == parse + run], "This file
//! used to hold ~60 lines running the three stages by hand, because
//! ID(pipeline/subject-equals-source-breaks-attributes) had concluded Tr(Pipeline) could not
//! express an attribute macro. It can - see NOTE(#pipeline-macro/is-a-pipeline) for why the
//! original diagnosis was wrong - so what is left here is the only thing rustc's signature forces:
//! turn two token streams into a node, and turn what comes back into one.
//!
//! That is the same shape every entry function Attr(pipeline) GENERATES has. The macro and the
//! macros it writes now differ in no way that matters, which is the proof the abstraction is real
//! rather than merely present."

pub(crate) mod extractor;
pub(crate) mod generator;
pub(crate) mod processor;

use proc_macro2::TokenStream;
use proc_macro_flow_traits::attributed::Attributed;
use proc_macro_flow_traits::pipeline::Pipeline;
use quote::ToTokens;
use syn::ItemMod;

use extractor::{PipelineExtraction, PipelineSource};
use generator::{PipelineArgs, PipelineExpansion};

/// The wiring this macro runs through - the same marker type it generates for everyone else.
pub(crate) struct PipelineWiring;

impl<'ast> Pipeline<'ast> for PipelineWiring {
    type Extractor = PipelineExtraction<'ast>;
    /// Its own extractor, which ID(pipeline/no-processor-is-the-extractor) permits and which is
    /// what the commonest pipeline looks like.
    type Processor = PipelineExtraction<'ast>;
    type Generator = PipelineExpansion;
}

/// Parse both inputs, run, lower.
///
/// `run` and not `run_attribute`: this macro REWRITES the module it was applied to - the stripped
/// module is the first thing S(PipelineExpansion) emits - so handing the item back as well would
/// emit it twice. That is `emits = replace` (NOTE(#pipeline-macro/emission-must-be-declared)),
/// declared here by being written this way.
pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let module = match syn::parse2::<ItemMod>(item.clone()) {
        Ok(module) => module,
        // The author's item is NOT deleted on a parse failure - an attribute macro replaces what
        // it annotates, so emitting only the error would delete their code.
        Err(error) => return with(item, error),
    };

    let args = match syn::parse2::<PipelineArgs>(attr) {
        Ok(args) => args,
        Err(error) => return with(module.to_token_stream(), error),
    };

    let node: PipelineSource = Attributed::new(&args, &module);
    PipelineWiring::run(node).to_token_stream()
}

/// Emit what was built, then the complaint.
fn with(built: TokenStream, error: syn::Error) -> TokenStream {
    let mut out = built;
    out.extend(error.to_compile_error());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro_flow_traits::extractor::Validate;
    use quote::quote;

    /// The claim of NOTE(#pipeline-macro/is-a-pipeline), asserted at the type level.
    ///
    /// Tr(Pipeline) binds all three stages to agree, so an impl existing at all is the proof that
    /// the macro's stages line up the way every generated pipeline's must. This function is never
    /// called; it not compiling is the failure.
    #[allow(dead_code)]
    fn the_macro_is_a_pipeline<'ast>() {
        fn requires_a_pipeline<'ast, P: Pipeline<'ast>>() {}
        requires_a_pipeline::<PipelineWiring>();
    }

    #[test]
    fn the_source_carries_both_halves_of_the_invocation() {
        // ID(attributed/source-is-a-pair). Before this the macro's own arguments were parsed in a
        // bespoke step outside the stages; now they ARE the source, like the module.
        // `mod m;` and not `mod m {}` - the latter has an EMPTY body, which validates fine.
        let module: ItemMod = syn::parse_str("mod m;").expect("parses");
        let args: PipelineArgs = syn::parse2(quote!(derive = Thing)).expect("parses");
        let node = Attributed::new(&args, &module);

        // `validate` narrows a module with no body to a complaint, and it reaches the module
        // THROUGH the pair - which is the whole shape change.
        assert!(PipelineExtraction::validate(node).is_err());
        assert_eq!(node.item().ident, "m");
        assert_eq!(node.args().exported, "Thing");
    }

    #[test]
    fn running_the_pipeline_emits_the_module_and_its_wiring() {
        let out = expand(quote!(derive = Thing), quote! {
            mod stages {
                #[extractor(source = DeriveInput)] struct Read;
                #[processor(from = Read)] struct Understood;
                #[generator(from = Understood)] struct Built;
            }
        })
        .to_string();

        assert!(out.contains("mod stages"), "{out}");
        assert!(out.contains("struct ThingWiring"), "{out}");
        assert!(out.contains("proc_macro_derive"), "{out}");
    }

    #[test]
    fn the_module_is_emitted_exactly_once() {
        // `run`, not `run_attribute` - this macro REWRITES the module, so re-emitting it as well
        // would define `mod stages` twice. See NOTE(#pipeline-macro/emission-must-be-declared).
        let out = expand(quote!(derive = Thing), quote! {
            mod stages {
                #[extractor(source = DeriveInput)] struct Read;
                #[processor(from = Read)] struct Understood;
                #[generator(from = Understood)] struct Built;
            }
        })
        .to_string();

        assert_eq!(out.matches("mod stages").count(), 1, "{out}");
    }

    #[test]
    fn a_complaint_is_emitted_exactly_once() {
        // REGRESSION for NOTE(#processor/reasons-are-new-not-inherited). Six processors copied
        // their input's reasons forward, and F(run) had already rendered those from the tree, so
        // every diagnostic in the crate came out TWICE. Nothing caught it because every test
        // asserted a complaint was present and none asserted how many.
        let out = expand(quote!(derive = Thing), quote! {
            mod stages {
                #[extractor(source = DeriveInput)] struct Read;
                #[processor(from = Nope)] struct Understood;
                #[generator(from = Understood)] struct Built;
            }
        })
        .to_string();

        // `compile_error !`, the MACRO - the generated entry body legitimately calls
        // `error.to_compile_error()`, which contains the bare substring.
        assert_eq!(out.matches("compile_error !").count(), 1, "{out}");
    }

    #[test]
    fn a_module_that_does_not_parse_keeps_the_authors_code() {
        let out = expand(quote!(derive = Thing), quote!(fn not_a_module() {})).to_string();

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("fn not_a_module"), "the author's item was deleted: {out}");
    }

    #[test]
    fn arguments_that_do_not_parse_keep_the_module() {
        let out = expand(quote!(nonsense), quote!(mod m {})).to_string();

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("mod m"), "the module was deleted: {out}");
    }
}
