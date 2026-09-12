// @review [~]
use crate::{
    StructExtraction,
    base::extractor::extractor::{attribute::TransformationExtraction, field::FieldExtraction},
};

// Query(#processor/base-scope):Q[this ??], "What should the base ExtractorProcessor/FieldProcessor/TransformationProcessor actually validate or transform before generation, versus what's left for a concrete processor built on top? Decide before wiring #processor/expansion in extractor/mod.rs"
// Answer(#processor/scope-reply):A[ID(processor/base-scope) ==? ID(syntax/extraction)],
//   "The validate half is answered: the base processor validates NOTHING. Validation is the syntax
//   stage's job and its output is already a tree of Extraction<T> carrying its own reasons, so a
//   processor that re-validates would be duplicating a check it cannot improve on and discarding
//   the spans that make the result diagnosable. What is left for the base is TRANSFORM only -
//   narrowing a well-formed extraction into whatever the generator wants to consume. The concrete-
//   processor half stays open until ID(processor/expansion) forces it"
pub struct ExtractorProcessor<'ast> {
    source: StructExtraction<'ast>,
}

pub struct FieldProcessor<'ast> {
    source: FieldExtraction<'ast>,
}

pub struct TransformationProcessor<'ast> {
    source: TransformationExtraction<'ast>,
}
