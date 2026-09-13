// @review [ ]
use crate::traits::visitable::Visitable;

pub(crate) mod extractor;
pub(crate) mod generator;
pub(crate) mod processor;
pub(crate) mod visitable;

// TODO(#pipeline/relocate-traits):M[Tr(Pipeline) => N(proc_macro_flow_traits)], "This crate has proc-macro = true, so pub items here (Pipeline, Validate, Extractor, and Processor/Generator once they exist) can't be depended on by any other crate - proc_macro_flow_traits is already an ordinary lib and already a dependency of this crate, so the base trait definitions belong there; StructExtraction/FieldExtraction/etc. here should just implement them"
// BLOCKED(#pipeline/traits):V[ID(pipeline/base-processor) ==? this] && V[ID(pipeline/base-generator) ==? this], "Commented out, not deleted. Processor and Generator are not defined ANYWHERE in the workspace, so this signature never compiled - and it cannot be written here, because the two traits belong in proc_macro_flow_traits (a proc-macro crate exports nothing but macros). Restore it there, with both parameters of Extractor supplied, once those land"
// pub trait Pipeline<'ast, E: Extractor<'ast, I>, P: Processor<'ast>, G: Generator<'ast>> {}

pub trait Validate<'ast, I: Visitable<'ast>> {
    // Answer(#pipeline/validity-error):A[ID(syntax/reason) ==? this], "Was #helper, with an unquoted message that never parsed as a task. Answered: ValidityError should not be bounded by std::error::Error, it should stop being an associated type at all. Failures become a Reason recorded on the node (ID(syntax/extraction)), because a proc macro only ever EMITS an error - it never handles one - so a per-type error buys nothing and cannot combine with a sibling's, which is what accumulation needs"
    type ValidityError;
    type Valid;

    fn validate(input: I) -> Result<Self::Valid, Self::ValidityError>;
}
