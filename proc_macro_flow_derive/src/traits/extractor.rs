// @review [ ]
use proc_macro_flow_traits::{extractor::Extraction, source::Sourced};

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
pub(crate) trait Extractor<'ast, I: Visitable<'ast>>:
    Sized + Sourced<'ast> + Validate<'ast, I>
{
    // `node: I`, not `&'ast I`. `I` is the BORROWED node type (`&'ast DeriveInput`, not
    // `DeriveInput`), which is what Validate already assumes in `validate(input: I)` - taking a
    // reference to it again gave `&'ast &'ast DeriveInput`, and that double reference is what the
    // now-deleted phantom `type Node` existed to paper over.
    fn extract_from(node: I) -> Extraction<Self>;
}
