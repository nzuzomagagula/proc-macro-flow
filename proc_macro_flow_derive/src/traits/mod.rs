// @review [ ]
use crate::traits::visitable::Visitable;

pub(crate) mod extractor;
pub(crate) mod generator;
pub(crate) mod processor;
pub(crate) mod visitable;

// TODO(#pipeline/relocate-traits):M[Tr(Pipeline) => N(proc_macro_flow_traits)], "This crate has proc-macro = true, so pub items here can't be depended on by any other crate - proc_macro_flow_traits is already an ordinary lib and already a dependency, so the base trait definitions belong there and StructExtraction/FieldExtraction/etc. here should just implement them. STATUS: Extraction, Extracted, Reason, Resolution and the meta openings have all moved; Validate and Extractor have NOT, and the urgency dropped when the stage boundary was corrected - they are implemented only by this crate's own stages now, not by author grammar types, so nothing outside needs to name them. Pipeline is commented out entirely at ID(pipeline/traits), pending Processor and Generator existing at all"
// BLOCKED(#pipeline/traits):V[ID(pipeline/base-processor) ==? this] && V[ID(pipeline/base-generator) ==? this], "Commented out, not deleted. Processor and Generator are not defined ANYWHERE in the workspace, so this signature never compiled - and it cannot be written here, because the two traits belong in proc_macro_flow_traits (a proc-macro crate exports nothing but macros). Restore it there, with both parameters of Extractor supplied, once those land"
// pub trait Pipeline<'ast, E: Extractor<'ast, I>, P: Processor<'ast>, G: Generator<'ast>> {}

// Answer(#pipeline/validity-scope):A[ID(pipeline/validity-error) ==? this], "The trait was never
// vestigial, it was UNDEFINED - which is why all three impls are Ok(input) and why deleting it kept
// looking tempting. Its job is SURFACE-LEVEL validation and nothing more: does this exist where it is
// required, is this value within some bound - the questions answerable by looking at a node without
// interpreting it. It must NOT parse and it must not read grammar; a node's meaning belongs to the
// processor. That also settles the sibling question at ID(pipeline/validity-error): failures here
// become a Reason on the node, because a surface check has a span and a cause and nothing else"
pub trait Validate<'ast, I: Visitable<'ast>> {
    // Answer(#pipeline/validity-error):A[ID(syntax/reason) ==? this], "Was #helper, with an unquoted message that never parsed as a task. Answered: ValidityError should not be bounded by std::error::Error, it should stop being an associated type at all. Failures become a Reason recorded on the node (ID(syntax/extraction)), because a proc macro only ever EMITS an error - it never handles one - so a per-type error buys nothing and cannot combine with a sibling's, which is what accumulation needs"
    type ValidityError;
    type Valid;

    fn validate(input: I) -> Result<Self::Valid, Self::ValidityError>;
}
