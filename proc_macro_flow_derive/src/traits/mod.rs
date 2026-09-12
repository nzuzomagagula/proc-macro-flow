use std::error::Error;

// @review [ ]
use crate::traits::{extractor::Extractor, visitable::Visitable};

pub(crate) mod extractor;
pub(crate) mod generator;
pub(crate) mod processor;
pub(crate) mod visitable;

// TODO(#pipeline/relocate-traits):M[Tr(Pipeline) => N(proc_macro_flow_traits)], "This crate has proc-macro = true, so pub items here (Pipeline, Validate, Extractor, and Processor/Generator once they exist) can't be depended on by any other crate - proc_macro_flow_traits is already an ordinary lib and already a dependency of this crate, so the base trait definitions belong there; StructExtraction/FieldExtraction/etc. here should just implement them"
pub trait Pipeline<'ast, E: Extractor<'ast>, P: Processor<'ast>, G: Generator<'ast>> {}

pub trait Validate<'ast, I: Visitable<'ast>> {
    // Answer(#pipeline/validity-error):A[ID(syntax/reason) ==? this], "Was #helper, with an unquoted message that never parsed as a task. Answered: ValidityError should not be bounded by std::error::Error, it should stop being an associated type at all. Failures become a Reason recorded on the node (ID(syntax/extraction)), because a proc macro only ever EMITS an error - it never handles one - so a per-type error buys nothing and cannot combine with a sibling's, which is what accumulation needs"
    type ValidityError;
    type Valid;

    fn validate(input: I) -> Result<Self::Valid, Self::ValidityError>;
}
