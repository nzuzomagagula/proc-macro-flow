// @review [~]
use crate::{
    StructExtraction,
    base::extractor::extractor::{attribute::TransformationExtraction, field::FieldExtraction},
};

// Moved here from base/extractor/extractor/mod.rs - these describe the PROCESSOR stage and had
// no reason to sit in the extractor module.
// TODO(#processor/pipeline):C[S(ProcessorPipeline)], "Bare struct holding its own extractor/processor/generator triple for the processor stage"
// TODO(#processor/expansion):C[F(expand)], "expand(&ProcessorPipeline) -> TokenStream first, concretely; only then wire the outer expansion of the Visit impls this stage drives (visit_field/visit_fields/visit_attribute over FieldExtraction/StructExtraction/TransformationExtraction). Two separate passes - don't conflate the inner macro-of-a-macro with the outer traversal"
// TODO(#processor/macro):C[F(processor)], "Proc-macro entry point for the processor stage, alongside lib.rs::extractor"
// TODO(#processor/helpers):C[Tr(ProcessorHelpers)], "Shared trait/helpers so concrete processors (visit_field, visit_fields, visit_attribute, and future ones) don't reimplement Visit boilerplate for per-field/per-attribute customisation"
// Query(#processor/base-scope):Q[this ??], "What should the base ExtractorProcessor/FieldProcessor/TransformationProcessor actually validate or transform before generation, versus what's left for a concrete processor built on top? Decide before wiring #processor/expansion in extractor/mod.rs"
// Answer(#processor/scope-reply):A[ID(processor/base-scope) ==? ID(syntax/extraction)], "The validate half is answered: the base processor validates NOTHING. Validation is the syntax stage's job and its output is already a tree of Extraction<T> carrying its own reasons, so a processor that re-validates would be duplicating a check it cannot improve on and discarding the spans that make the result diagnosable. What is left for the base is TRANSFORM only - narrowing a well-formed extraction into whatever the generator wants to consume. The concrete- processor half stays open until ID(processor/expansion) forces it"
pub struct ExtractorProcessor<'ast> {
    source: StructExtraction<'ast>,
}

pub struct FieldProcessor<'ast> {
    source: FieldExtraction<'ast>,
}

pub struct TransformationProcessor<'ast> {
    source: TransformationExtraction<'ast>,
}
