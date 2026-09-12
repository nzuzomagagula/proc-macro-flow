// @review [ ]
// TODO(#generator/pipeline):C[S(GeneratorPipeline)], "Bare struct holding its own extractor/processor/generator triple for the generator stage"
// TODO(#generator/expansion):C[F(expand)], "expand(&GeneratorPipeline) -> TokenStream first, concretely; only then wire the outer expansion that emits the final generated code (the impl/TokenStream the StructExtraction pipeline ultimately produces). Two separate passes - don't conflate the inner macro-of-a-macro with the outer codegen"
// TODO(#generator/macro):C[F(generator)], "Proc-macro entry point for the generator stage, alongside lib.rs::extractor"

use syn::ItemImpl;

use crate::base::extractor::processor::ExtractorProcessor;

// Query(#generator/base-scope):Q[this ??], "What should the base Generator::generate_visitor actually emit - just the ItemImpl shape, or the full derive expansion body? Decide before wiring #generator/expansion"
// Answer(#generator/scope-reply):A[ID(generator/base-scope) ==? ID(syntax/render)],
//   "Partially answered by the syntax stage: whatever it emits, it must emit it EVEN WHEN the
//   extraction failed. A generator that returns only compile_error!s leaves the impl missing, and
//   the resulting 'does not implement' cascade at every use site buries the real diagnostic. So the
//   floor is a stub ItemImpl beside the errors. Whether the base also emits the full body, or only
//   the shape for a concrete generator to fill, stays open until ID(generator/expansion)"
pub struct Generator<'ast> {
    processor: ExtractorProcessor<'ast>,
}

impl<'ast> Generator<'ast> {
    // TODO[ ](#generator/visitor):U[F(generate_visitor)], "target: () and unimplemented!() are placeholders so the crate compiles - needs a real target type (likely &ProcessorPipeline or &StructExtraction) and a body that emits the ItemImpl"
    fn generate_visitor(target: ()) -> ItemImpl {
        let _ = target;
        unimplemented!()
    }
}
