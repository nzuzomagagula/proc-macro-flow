// @review [ ]
// TODO(#processor/pipeline):C[S(ProcessorPipeline)], "Bare struct holding its own extractor/processor/generator triple for the processor stage"
// TODO(#processor/expansion):C[F(expand)], "expand(&ProcessorPipeline) -> TokenStream first, concretely; only then wire the outer expansion of the Visit impls this module already drives (visit_field/visit_fields/visit_attribute over FieldExtraction/StructExtraction/TransformationExtraction). Two separate passes - don't conflate the inner macro-of-a-macro with the outer traversal"
// TODO(#processor/macro):C[F(processor)], "Proc-macro entry point for the processor stage, alongside lib.rs::extractor"
// TODO(#processor/helpers):C[Tr(ProcessorHelpers)], "Shared trait/helpers so concrete processors (visit_field, visit_fields, visit_attribute, and future ones) don't reimplement Visit boilerplate for per-field/per-attribute customisation"
// TODO(#extractor/pipeline):C[S(ExtractorPipeline)], "Bare struct holding its own extractor/processor/generator triple, mirroring the StructExtraction pipeline this file already builds - the extractor stage becomes self-hosting"
// TODO(#extractor/expansion):C[F(expand)], "expand(&ExtractorPipeline) -> TokenStream first, concretely; only then wire the outer expansion (ExtractionState<StructExtraction>::visit_derive_input over DeriveInput/ItemStruct). Two separate passes - don't conflate the inner macro-of-a-macro with the outer traversal already in processor.rs"
// TODO[~](#extractor/macro):U[F(extractor)], "lib.rs::extractor now compiles but returns an empty TokenStream() as a placeholder - finish it once ExtractorPipeline::expand exists"
use syn::{Fields, visit::Visit};

use crate::base::extractor::extractor::field::FieldExtraction;
pub(crate) use crate::traits::extractor::ExtractionState;
pub mod attribute;
pub mod field;

// TODO[x](#cleanup):U[this, "This should become a typestate instead of a runtime enum"]

pub(crate) struct StructExtraction<'ast> {
    pub(crate) fields: Vec<ExtractionState<FieldExtraction<'ast>>>,
}

//Fix[ ](#extractor/macro):U[this, "When expanding the Extractors, future macros should not use their children's genesis syn types (like Fields here) and instead use their *own* source type, to keep things recursive and should find their children from there"]
impl<'ast> Visit<'ast> for ExtractionState<StructExtraction<'ast>> {
    fn visit_fields(&mut self, i: &'ast Fields) {
        let fields = i
            .iter()
            .map(ExtractionState::<FieldExtraction<'_>>::extract)
            .collect();
        *self = ExtractionState::Initialised(StructExtraction { fields });
    }
}
