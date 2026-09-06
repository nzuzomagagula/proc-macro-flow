// @review [ ]
// TODO(#generator/pipeline):C[S(GeneratorPipeline)], "Bare struct holding its own extractor/processor/generator triple for the generator stage"
// TODO(#generator/expansion):C[F(expand)], "expand(&GeneratorPipeline) -> TokenStream first, concretely; only then wire the outer expansion that emits the final generated code (the impl/TokenStream the StructExtraction pipeline ultimately produces). Two separate passes - don't conflate the inner macro-of-a-macro with the outer codegen"
// TODO(#generator/macro):C[F(generator)], "Proc-macro entry point for the generator stage, alongside lib.rs::extractor"

use syn::ItemImpl;

use crate::base::extractor::processor::ExtractorProcessor;

pub struct Generator<'ast>{
    processor: ExtractorProcessor<'ast>,   
}

impl<'ast> Generator<'ast> {
    // TODO[~](#generator/visitor):U[F(generate_visitor)], "target: () and unimplemented!() are placeholders so the crate compiles - needs a real target type (likely &ProcessorPipeline or &StructExtraction) and a body that emits the ItemImpl"
    fn generate_visitor(target: ()) -> ItemImpl {
        let _ = target;
        unimplemented!()
    }
}
