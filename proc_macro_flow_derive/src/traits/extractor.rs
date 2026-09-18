// @review [ ]
use crate::traits::{Validate, visitable::Visitable};

// TODO[x](#cleanup):R[E(ExtractionState) -> S(Extraction)], "RESOLVED, and now LANDED in
// proc_macro_flow_traits::extractor - not as a typestate. The answer is { value: Option<T>,
// reasons: Vec<Reason> }: a typestate cannot express 'this node extracted fine AND has a complaint
// of its own', which is what an unknown key is - a failure of the PARENT to consume its input, with
// the value still perfectly good. Two states could not carry a reason at all. Note the CONTRAST
// with resolution::Stage, which IS a typestate precisely because resolved/unresolved has no such
// second axis"
// TODO[x](#extractor/no-result):U[F(extract_from)], "extract_from returns Extraction<Self> and no
// longer a Result, so there is no `?`, no early return, and no way to drop a sibling on the way
// out. The per-type ExtractionError associated types went with it - a proc macro only ever EMITS
// an error, so a taxonomy of error structs bought nothing and actively fought accumulation"
// NOTE(#extractor/two-questions): V[Tr(Extractor).sup(Visitable) && S(Extracted).P(source)], "The
// stage still answers exactly two questions - what is the SOURCE of this extraction, and how do we
// GET THERE - but only one of them is a supertrait now. `I: Visitable` answers the second. The
// first is answered STRUCTURALLY by Extracted, which cannot be built without a source, rather than
// by Tr(Sourced), which required every implementor to store one and hand it back honestly.
// VERIFIED that the trait earned nothing: it had a single real caller, in a test, while
// Extracted::source covered every other site AND survived a failed extraction, where there is no
// Self to ask. Storing the node as well was a second answer to one question.
// There is still deliberately no third question about how to PARSE the node - an extractor may hand
// on a raw TokenStream and leave understanding it to the processor, which is why Ty(Output) below
// is unconstrained"
pub(crate) trait Extractor<'ast, I: Visitable<'ast>>: Sized + Validate<'ast, I> {
    /// What the processor receives.
    ///
    /// TODO[ ](#extractor/output-bound):C[Ty(Output).bound], "Unbounded ON PURPOSE. The honest bound
    /// is 'something the processor can consume', and Tr(Processor) is a comment-only stub in both
    /// crates - a bound written now would encode a guess about its needs that nothing could falsify.
    /// Callers pin Output themselves in the meantime, which is still a compile error when wrong.
    /// Lands with ID(pipeline/base-processor)"
    type Output;

    // `node: I`, not `&'ast I`. `I` is the BORROWED node type (`&'ast DeriveInput`, not
    // `DeriveInput`), which is what Validate already assumes in `validate(input: I)` - taking a
    // reference to it again gave `&'ast &'ast DeriveInput`, and that double reference is what the
    // now-deleted phantom `type Node` existed to paper over.
    fn extract_from(node: I) -> Self::Output;
}

/* @group(#from)
 *
 * What `#[from = ..]` lowers to. An extraction designer should declare WHERE a field comes from and
 * never how to walk to it, so the three functions below are the whole injection surface: the derive
 * reads the declared path, picks one of them by the field's TYPE, and emits the call.
 *
 * NOTE(#from/names-its-target): V[F(extract_each).turbofish], "Call sites name T explicitly, and
 * cannot avoid it: the return type is Vec<T::Output>, and an associated type is not injective, so
 * nothing lets rustc work backwards from the field's type to the extractor that produces it. That
 * is a cost of Ty(Output) being free-form (ID(extractor/output-bound)) and it is the right trade -
 * generated code always knows T, so the turbofish is written by the derive and read by nobody. Do
 * not 'fix' it by pinning Output to Extracted; that would buy inference with the flexibility the
 * processor stage was promised"
 *
 * NOTE(#from/arity-from-type): V[T(Vec) => F(extract_each)] && V[T(Option) => F(extract_maybe)],
 * "Which helper a #[from] lowers to is read off the field's type, never off the attribute. That is
 * the same rule the grammar already uses for requiredness and repetition - T required, Option<T>
 * optional, Vec<T> repeated - and it is what stops #[from] growing a second vocabulary for arity
 * that could disagree with the type it sits on"
 *
 * TODO[ ](#from/attribute):C[Attr(from)], "The derive half. Takes an EXPRESSION, in two forms that
 * cover the common case: a field path - `#[from = source.data.fields]` - and a simple closure -
 * `#[from = |source| source.attrs.iter().filter(..)]`. Either way the field's TYPE picks the helper
 * below, and the expression is spliced verbatim into generated code, so a bad one is rustc's error
 * at the AUTHOR's span in the author's own crate. That is the same bargain ID(no-parse) already
 * takes for #[shape(..)]: emit it, do not interpret it"
 *
 * NOTE(#from/not-total): V[Attr(from).optional], "#[from] is CONVENIENCE and is not required to be
 * sufficient. Extraction that needs real logic - correlating two sources, conditioning on something
 * the path cannot see - writes extract_from by hand, which stays fully available and is what every
 * extractor in this crate does today. The attribute exists so the easy majority stops being written
 * out longhand, not so the hard minority becomes expressible in an attribute. Resist growing it a
 * vocabulary for the latter; that is the darling failure mode in a different costume"
 *
 * Query(#from/native-and-custom): Q[F(extract_each).A(\1).T(I) ??], "These are generic over
 * I: Visitable, so a syn node works today. A CUSTOM grammar node as a source needs Tr(Visitable)
 * generalised over the visitor family - PROVEN to work (one walk() drove a syn node and a grammar
 * node in the same shape) but not yet landed, because nothing needed it until #[from] did. That is
 * the prerequisite for 'native nodes and our own nodes as sources' being one mechanism"
 */

/// One child, extracted from one node.
// Unused until ID(from/attribute) generates the calls - `extract_each` is the only arity the two
// hand-written extractors happen to need. Kept rather than deferred so the lowering target is one
// complete surface, not three functions arriving one at a time as the derive learns each arity.
#[allow(dead_code)]
pub(crate) fn extract<'ast, T, I>(node: I) -> T::Output
where
    T: Extractor<'ast, I>,
    I: Visitable<'ast>,
{
    T::extract_from(node)
}

/// Many children. The source is anything iterable, which is what a `Vec` field declares.
pub(crate) fn extract_each<'ast, T, I, N>(nodes: N) -> Vec<T::Output>
where
    T: Extractor<'ast, I>,
    I: Visitable<'ast>,
    N: IntoIterator<Item = I>,
{
    nodes.into_iter().map(T::extract_from).collect()
}

/// A child that may not be there. Absence is not a failure and records no reason - the field's
/// `Option` is what says so, and something that is allowed to be missing has nothing to complain
/// about when it is.
#[allow(dead_code)]
pub(crate) fn extract_maybe<'ast, T, I>(node: Option<I>) -> Option<T::Output>
where
    T: Extractor<'ast, I>,
    I: Visitable<'ast>,
{
    node.map(T::extract_from)
}
