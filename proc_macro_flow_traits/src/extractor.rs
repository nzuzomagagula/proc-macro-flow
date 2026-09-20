// @review [ ]
//! `Extraction<T>` - a value, the complaints attached to it, or both.
//!
//! Answers ID(cleanup): the shape is `{ value: Option<T>, reasons: Vec<Reason> }` and NOT a
//! typestate, because a node can extract perfectly well AND carry a complaint of its own - an
//! unknown key is the parent failing to consume its input while the value stays good. Two states
//! could not say that. Contrast ID(no-idempotence) in `resolution`, where resolved/unresolved has
//! no second axis and therefore IS a typestate.
//!
//! NOTE(#no-owned-nodes): V[crate.!owns(T(TokenStream))], "THIS CRATE DOES NOT OWN AST NODES. Every
//! type here holds `&'ast` into the syntax tree the macro was handed - Unresolved and Resolved hold
//! &'ast TokenStream, Extracted holds its input node, the stage extractors hold their own. Reason
//! was the one exception, keeping an owned TokenStream snapshot, and it was justified here as 'the
//! error path, so the clone costs nothing'. That was a rationalisation for avoiding a lifetime
//! parameter. Two things were wrong with it: a snapshot can silently diverge from the node it was
//! taken from, and the borrow would have been valid anyway - everything descends from the
//! DeriveInput, which outlives the whole expansion INCLUDING the final render pass. Reason now
//! stores a Span (see ID(reason/span-not-node)), which owns nothing and needs no lifetime at all"
//!
//! NOTE(#no-result): V[F(extract_from).R(Extraction) != R(Result)], "extract_from returns
//! Extraction<Self>, never Result. With no `?` there is no early return, and with no early return
//! there is no way to drop a sibling or lose a position on the way out. The enforcement is
//! structural rather than a convention somebody has to hold to"

use proc_macro2::Span;
use quote::ToTokens;
use syn::spanned::Spanned;

use crate::visitable::Visitable;

pub struct Extraction<T> {
    pub value: Option<T>,
    pub reasons: Vec<Reason>,
}

/// Why a node is unhappy, and - when it can be more precise than the node - where.
///
/// NOTE(#reason/span-not-node): V[S(Reason).P(span).T(Span)], "A Reason stores a SPAN while
/// Extracted stores a NODE, and the asymmetry is deliberate rather than an oversight. ID(source-
/// not-span) rejected a stored Span because Span::join is nightly-only, so one taken over a
/// multi-token node collapses to its first token - true, and why the node's own source is kept as a
/// node and rendered with Error::new_spanned. A reason's finer pointer is a different animal: it
/// points at ONE token (`colur`, a duplicated key, a variant name), and a single token's span needs
/// no joining, so it is exact. Where it is not - `Other(Blue, Teal)` - first-token still lands on
/// `Other`, which is the right place anyway.
///
/// This also settles a design question the earlier draft got wrong. A per-reason source cannot be
/// `&'ast I` for the node's own `I`: the offending token is always a DIFFERENT type from the node
/// (an Ident or Path inside an Attribute), so the parameter could not be made to fit. The options
/// were therefore erasure or a Span, and a Span costs no dyn, no lifetime on Reason, and - the part
/// that matters - no lifetime on Extraction, which would otherwise have propagated through every
/// holder in the crate.
///
/// `None` means 'I have nothing finer to say than the node I sit on', and the renderer falls back
/// to the reference chain: Extracted holds the node, so ID(no-ancestry) still supplies position."
pub struct Reason {
    pub kind: ReasonKind,
    span: Option<Span>,
}

/// CLOSED reasons, open rendering - closed so the framework can interpret what it caught and render
/// it against the node table, `Custom` so an exotic grammar is never blocked outright.
pub enum ReasonKind {
    WrongShape,
    UnknownKey,
    Missing,
    Duplicate,
    Ambiguous,
    Custom(String),
}

impl<T> Extraction<T> {
    pub fn value(value: T) -> Self {
        Self {
            value: Some(value),
            reasons: Vec::new(),
        }
    }

    pub fn failed(reason: Reason) -> Self {
        Self {
            value: None,
            reasons: vec![reason],
        }
    }

    pub fn with_reason(mut self, reason: Reason) -> Self {
        self.reasons.push(reason);
        self
    }

    pub fn is_failure(&self) -> bool {
        self.value.is_none()
    }

    /// Take another extraction's reasons and hand back its value.
    ///
    /// This is the primitive a parent uses to gather children: the reasons always come across,
    /// whether or not the child produced anything, so no sibling's complaint is lost on a path
    /// where its value was.
    pub fn absorb<U>(&mut self, other: Extraction<U>) -> Option<U> {
        self.reasons.extend(other.reasons);
        other.value
    }
}

impl<T> Default for Extraction<T> {
    fn default() -> Self {
        Self {
            value: None,
            reasons: Vec::new(),
        }
    }
}

impl Reason {
    /// A complaint about the node this reason will sit on.
    pub fn new(kind: ReasonKind) -> Self {
        Self { kind, span: None }
    }

    /// A complaint about one specific token, which is finer than the node can point at.
    ///
    /// This is the `colur(Red)` / duplicate-key case: recorded on the PARENT, because no child owns
    /// the offending token, but wanting to underline the token rather than the whole parent.
    pub fn at<S: Spanned + ?Sized>(kind: ReasonKind, token: &S) -> Self {
        Self {
            kind,
            span: Some(token.span()),
        }
    }

    pub fn span(&self) -> Option<Span> {
        self.span
    }

    // TODO[ ](#reason/message):U[F(to_error)], "The message is a parameter today because deciding
    // it belongs to ID(diagnostics) crossed with ID(node-table), neither of which exists. What is
    // already settled is that it is rendered LATE, from kind plus tree position - baking a string
    // at record time is what would put it out of an author's reach"
    /// Render, falling back to `node` when this reason has nothing finer to point at.
    ///
    /// Taking the fallback as an argument is the point: a reason cannot render itself, because
    /// position belongs to where it sits in the tree, not to the reason.
    pub fn to_error(&self, node: &impl ToTokens, message: impl core::fmt::Display) -> syn::Error {
        match self.span {
            Some(span) => syn::Error::new(span, message),
            None => syn::Error::new_spanned(node, message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn a_reason_points_at_a_token_and_falls_back_to_the_node() {
        let node = quote!(configuration(colur(Red), name = "thing"));
        let token: syn::Ident = syn::parse_str("colur").unwrap();

        // Finer than the node: `at` records the offending token's own span. A single token needs no
        // Span::join, so this one is exact - which is the whole argument of
        // ID(reason/span-not-node).
        let precise = Reason::at(ReasonKind::UnknownKey, &token);
        assert!(precise.span().is_some());

        // Nothing finer to say: the renderer falls back to the node it sits on, and `new_spanned`
        // gives that node its full start..end range rather than just its first token.
        let coarse = Reason::new(ReasonKind::WrongShape);
        assert!(coarse.span().is_none());

        assert!(!precise
            .to_error(&node, "unknown key")
            .to_compile_error()
            .is_empty());
        assert!(!coarse
            .to_error(&node, "wrong shape")
            .to_compile_error()
            .is_empty());
    }

    #[test]
    fn a_reason_cannot_render_itself_without_the_chain() {
        // `to_error` takes the fallback node as an argument on purpose: position belongs to where a
        // reason SITS in the tree, not to the reason. A reason with no span and no node has nothing
        // to point at, and the signature makes that unrepresentable rather than merely discouraged.
        let node = quote!(colour(ColourSetting::Red));
        let reason = Reason::new(ReasonKind::Missing);

        assert!(!reason
            .to_error(&node, "missing required key")
            .to_compile_error()
            .is_empty());
    }

    #[test]
    fn absorb_takes_every_reason_even_from_a_child_that_produced_nothing() {
        let mut parent = Extraction::value("parent");

        let good: Extraction<u8> =
            Extraction::value(1).with_reason(Reason::at(ReasonKind::UnknownKey, &quote!(colur)));
        let bad: Extraction<u8> = Extraction::failed(Reason::new(ReasonKind::Missing));

        assert_eq!(parent.absorb(good), Some(1));
        assert_eq!(parent.absorb(bad), None);

        // Both complaints survive, including the one from the child that had no value - this is
        // the property ID(no-result) exists to protect.
        assert_eq!(parent.reasons.len(), 2);
        assert!(parent.value.is_some());
    }

    #[test]
    fn a_node_can_extract_fine_and_still_carry_a_complaint() {
        // The case a typestate could not express, and the reason Extraction is a struct: an
        // unknown key is the PARENT failing to consume its input, with the value still good.
        let extraction =
            Extraction::value(()).with_reason(Reason::at(ReasonKind::UnknownKey, &quote!(colur)));

        assert!(!extraction.is_failure());
        assert_eq!(extraction.reasons.len(), 1);
    }
}

/// An extraction together with the node it was read from.
///
/// This is an extractor's output, and it is the PROCESSOR's input - which is the
/// only thing the extraction stage needs to know about it. An extractor answers
/// two questions, "what is the source of this extraction" and "how do we get
/// there"; it never answers "what does this node mean". A stage may therefore
/// hand on raw tokens (see `resolution::Unresolved`) and leave understanding them
/// to the processor.
///
/// The lifetime is carried by `I` rather than declared separately: `I` is always a
/// borrowed node (`&'ast DeriveInput`) or a `Copy` wrapper over one, so a third
/// parameter would only restate what `I` already says.
///
/// NOTE(#extracted/source-when-absent): V[S(Extracted).P(source)], "The source
/// lives HERE and not behind T: Sourced, because a failed extraction has no T to
/// ask. Sourced answers 'where did this node come from' for a node that exists;
/// Extracted answers it for an extraction that may have produced nothing - which
/// is exactly the case a diagnostic needs most"
///
/// NOTE(#extracted/not-unforgeable): V[F(new).pub], "`new` is public, and it has
/// to be: the stage extractors that build these live in proc_macro_flow_derive,
/// so a crate-private constructor would put the type out of reach of every real
/// implementor. What this type buys is therefore that a source is STRUCTURALLY
/// present and that the stage is named in the type - not that an Extracted cannot
/// be fabricated. Do not lean on it as a capability token"
pub struct Extracted<T, I> {
    extraction: Extraction<T>,
    source: I,
}

impl<T, I> Extracted<T, I> {
    pub fn new(extraction: Extraction<T>, source: I) -> Self {
        Self { extraction, source }
    }

    /// The node this was read from, for a renderer that wants to span something.
    pub fn source(&self) -> &I {
        &self.source
    }

    pub fn extraction(&self) -> &Extraction<T> {
        &self.extraction
    }

    pub fn into_extraction(self) -> Extraction<T> {
        self.extraction
    }

    pub fn value(&self) -> Option<&T> {
        self.extraction.value.as_ref()
    }

    pub fn reasons(&self) -> &[Reason] {
        &self.extraction.reasons
    }
}

impl<T, I: ToTokens> Extracted<T, I> {
    /// Span a message under the whole source node.
    ///
    /// Available even when the extraction produced no value, which is the point of
    /// ID(extracted/source-when-absent).
    pub fn to_error(&self, message: impl core::fmt::Display) -> syn::Error {
        syn::Error::new_spanned(&self.source, message)
    }
}

// ===========================================================================
// THE EXTRACTION CONTRACT
// ===========================================================================

// Answer(#pipeline/validity-scope):A[ID(pipeline/validity-error) ==? this], "The trait was never
// vestigial, it was UNDEFINED - which is why all three impls are Ok(input) and why deleting it kept
// looking tempting. Its job is SURFACE-LEVEL validation and nothing more: does this exist where it is
// required, is this value within some bound - the questions answerable by looking at a node without
// interpreting it. It must NOT parse and it must not read grammar; a node's meaning belongs to the
// processor. That also settles the sibling question at ID(pipeline/validity-error): failures here
// become a Reason on the node, because a surface check has a span and a cause and nothing else"
/* @group(#multi-source)
 *
 * ONE extraction pipeline, SEVERAL source node kinds. The motivating case is a grammar that must
 * read the same information from, say, an ItemMod and an ItemFn - and the evolution case behind
 * it: a React-like syntax that starts with types as proxies for render parameters and later grows
 * function forms, where backward compatibility should be an ADDED impl rather than a rewrite.
 *
 * NOTE(#multi-source/free-at-the-trait): V[S(X<S>).impl(Extractor).each(S)], "VERIFIED FREE TODAY,
 * on these traits, unmodified. Make the EXTRACTION TYPE generic and let each monomorphisation
 * carry one source:
 *
 *     struct Decl<S>(PhantomData<S>);
 *     impl<'ast> Validate<'ast> for Decl<ItemMod> { type Source = &'ast ItemMod; type Valid = Declared<'ast>; .. }
 *     impl<'ast> Validate<'ast> for Decl<ItemFn>  { type Source = &'ast ItemFn;  type Valid = Declared<'ast>; .. }
 *
 * Both resolve with no turbofish and no annotation, and `Decl::<ItemMod>::extract_maybe(None)`
 * still resolves - because the TYPE carries the source, so each concrete type has exactly one impl.
 *
 * THE WARNING IS THE POINT OF THIS NOTE. The obvious way to 'add multi-source support' is to turn
 * Ty(Source) back into a trait parameter, `Extractor<'ast, I>`, so one type can have many impls.
 * That reintroduces the guessing game: with several impls, a call whose argument pins nothing fails
 * with `error[E0283]: type annotations needed` / `multiple impls satisfying X: Extractor<_> found`
 * - and `extract_maybe(None)` is an ordinary thing for the derive to emit for an Option field. See
 * NOTE(#pipeline/source-is-associated). The generic goes on the TYPE, never on the trait"
 *
 * NOTE(#multi-source/valid-must-be-shared): V[Ty(Valid).shared], "Multi-source is legitimate ONLY
 * when the sources normalise to one Ty(Valid). That is what makes the rest of the pipeline
 * source-agnostic for free: everything downstream - Attr(from) included, whose `source` binding is
 * the VALIDATED value - consumes Valid and never learns which node it came from.
 *
 * If they do not share one, you do not have one pipeline with two sources. You have two pipelines
 * sharing a name, which is worse than two types, because the shared name asserts a commonality
 * that is not there. The tell: Ty(Valid) growing Option<T> fields so one source can leave them
 * empty, until it keeps only what EVERY source can supply and each source loses the thing that
 * made it worth extracting. Same failure mode NOTE(#from/not-total) names, in a different costume"
 *
 * TODO[ ](#multi-source/source-list):C[Attr(source).list], "The derive half, and the only expensive
 * part - which is why the attribute design is worth settling before Attr(source) grows neighbours.
 * `#[source(A, B)]` emits one Extractor impl per source. It must also FORCE a hand-written
 * Tr(Validate): there is no sensible generated narrowing from two unrelated nodes to one Ty(Valid),
 * and that is fine - it is the case ID(derive/three-not-one) split Validate out for.
 *
 * Note what does NOT need solving: Attr(from) expressions do not have to typecheck against every
 * source, because they are written against Ty(Valid), not against the raw node"
 */

// NOTE(#pipeline/source-is-associated): V[Tr(Validate).Ty(Source) && !Tr(Extractor).P(I)], "The
// source is an ASSOCIATED TYPE, not a trait parameter, so it is DETERMINED BY Self and never
// inferred. This is the same principle Ty(Output), Ty(Input) and the Stage GAT already follow, and
// Tr(Extractor)/Tr(Validate) were the two that did not.
//
// VERIFIED that the parameter form made call sites guess. With `Extractor<'ast, I>` a type MAY
// have several impls, so rustc has to pick one from the argument - and where the argument pins
// nothing it cannot: `FE::extract_maybe(None)` and `FE::extract_each(empty())` both fail with
// `error[E0283]: type annotations needed` / `multiple impls satisfying FE: Extractor<_> found`.
// Both are ordinary things for a derive to emit for an Option field. It only ever compiled because
// every extraction type happens to have exactly one impl - a property nothing enforced.
// VERIFIED that the associated form resolves all three call shapes with no annotation.
//
// It also makes Attr(source(Ty)) map ONTO something: the attribute declares `type Source = &Ty`
// one-for-one, where before it filled in a parameter that the trait let vary independently.
// Ty(Source) lives on Tr(Validate) rather than Tr(Extractor) because that is the trait that reads
// the node first - ID(extractor/two-questions)'s 'what is the source' is now answered by a type"
pub trait Validate<'ast> {
    /// The node this extraction reads - what `#[source(Ty)]` declares.
    ///
    /// An ASSOCIATED TYPE, not a trait parameter, and the difference is load-bearing. See
    /// NOTE(#pipeline/source-is-associated).
    type Source: Visitable<'ast>;

    // Answer(#pipeline/validity-error):A[ID(syntax/reason) == this], "DONE, and the answer was the
    // one predicted: ValidityError should not be bounded by std::error::Error, it should stop
    // being an associated type at all. A proc macro only ever EMITS an error - it never handles
    // one - so a per-type error buys nothing and cannot combine with a sibling's, which is what
    // accumulation needs. VERIFIED before removing it: the associated type was written by five
    // impls and read by ZERO - every Err arm in the crate discarded it - and three of the five
    // already set it to `()` because there was nothing meaningful to name. A parameter that most
    // implementors fill with the unit type is not carrying information; it is asking every author
    // to invent a name for 'no'. See ID(extractor/error)"
    type Valid;

    /// Narrow the source, or say why it could not be.
    ///
    /// NOTE(#validate/reason-is-offered-not-imposed): V[F(validate).R(Reason) != F(extract_from).records],
    /// "Returning a Reason OFFERS one; it does not oblige the caller to record it. The obvious
    /// reading of `Result<_, Reason>` is the opposite, so it is written down here: a HAND-WRITTEN
    /// extract_from decides whether the reason reaches the tree, and the DERIVE always records it.
    ///
    /// The case that forces the distinction is ID(heads-are-rustcs). An attribute that is not ours
    /// gets no value AND NO COMPLAINT - SyntaxFieldAttributeExtraction drops validate's reason on
    /// the floor deliberately, because a doc comment is an attribute and every documented field
    /// would otherwise be an error. That was a real bug once. Anyone 'fixing' the discard to look
    /// consistent with this signature resurrects it"
    fn validate(input: Self::Source) -> Result<Self::Valid, Reason>;
}

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
pub trait Extractor<'ast>: Sized + Validate<'ast> {
    /// What the processor receives.
    ///
    /// TODO[x](#extractor/output-bound):C[Ty(Output).bound], "RESOLVED, and the answer is that no
    /// bound belongs here. It was left open waiting for Tr(Processor) to exist so the honest bound
    /// - 'something a processor can consume' - could be written. Processor now exists, and writing
    /// it would be wrong: an extractor does not know which processor will consume it, and nothing
    /// makes the relation one-to-one. The agreement is declared from the OTHER side, where a
    /// Processor names its Input. A bound here would assert a coupling that does not exist.
    /// See NOTE(#processor/output-needs-no-bound)"
    type Output;

    // `Self::Source` is the BORROWED node type (`&'ast DeriveInput`, not `DeriveInput`), which is
    // why nothing here takes `&Self::Source` - a reference to it again gave `&'ast &'ast
    // DeriveInput`, and that double reference is what the now-deleted phantom `type Node` existed
    // to paper over.
    fn extract_from(node: Self::Source) -> Self::Output;

    /// Many children. The source is anything iterable, which is what a `Vec` field declares.
    fn extract_each<N>(nodes: N) -> Vec<Self::Output>
    where
        N: IntoIterator<Item = Self::Source>,
    {
        nodes.into_iter().map(Self::extract_from).collect()
    }

    /// A child that may not be there.
    ///
    /// Absence is not a failure and records no reason - the field's `Option` is what says so, and
    /// something allowed to be missing has nothing to complain about when it is.
    fn extract_maybe(node: Option<Self::Source>) -> Option<Self::Output> {
        node.map(Self::extract_from)
    }
}

/* @group(#from)
 *
 * What `#[from(..)]` lowers to. An extraction designer should declare WHERE a field comes from and
 * never how to walk to it, so `extract_from` / `extract_each` / `extract_maybe` are the whole
 * injection surface: the derive reads the declared expression, picks one of them by the field's
 * TYPE, and emits the call.
 *
 * Two corrections this header carried for a while, kept because both were wrong in instructive
 * ways. It said `#[from = ..]`, which rustc REJECTS outright (ID(derive/list-not-name-value)); and
 * it said "the three functions BELOW", which stopped being true when they became provided methods
 * on Tr(Extractor) ABOVE - see NOTE(#pipeline/no-free-functions). A group header that points at
 * nothing is worse than none, because it reads as current.
 *
 * Fix[x](#from/names-its-target):R[F(extract_each) -> M(Extractor::extract_each)], "RESOLVED by
 * objectification, and the note that stood here was pessimistic. It said call sites must carry a
 * turbofish and 'cannot avoid it', because Vec<T::Output> is not injective so rustc cannot work
 * backwards from the field type to the extractor. The DIAGNOSIS was right and the CONCLUSION was
 * wrong: nothing has to be inferred if the extractor is NAMED, and as a provided method on
 * Tr(Extractor) the receiver path names it. `extract_each::<FieldExtraction, _, _>(it)` becomes
 * `FieldExtraction::extract_each(it)`. Ty(Output) stays free-form, so none of the flexibility the
 * processor stage was promised is spent buying this. The general rule it is an instance of is
 * NOTE(#pipeline/no-free-functions)"
 *
 * NOTE(#from/arity-from-type): V[T(Vec) => F(extract_each)] && V[T(Option) => F(extract_maybe)],
 * "Which helper a #[from] lowers to is read off the field's type, never off the attribute. That is
 * the same rule the grammar already uses for requiredness and repetition - T required, Option<T>
 * optional, Vec<T> repeated - and it is what stops #[from] growing a second vocabulary for arity
 * that could disagree with the type it sits on"
 *
 * TODO[x](#from/attribute):C[Attr(from)], "DONE, in proc_macro_flow_derive::derive::extractor, and
 * the bargain held: the expression is spliced verbatim, so a bad one is rustc's error at the
 * AUTHOR's span in the author's own crate - ID(no-parse)'s deal for Attr(shape), taken again.
 *
 * TWO THINGS THIS ASKED FOR ARE NOT WHAT SHIPPED. The syntax is `#[from(expr)]`, not
 * `#[from = expr]`: VERIFIED that rustc rejects the name-value form with `attribute value must be
 * a literal`, so the expression never reaches the macro at all (ID(derive/list-not-name-value)).
 * And the two FORMS split into two ATTRIBUTES. A field path evaluates TO a value while a closure
 * is APPLIED to one, and telling `(expr)` from `(expr)(source)` apart requires inspecting the
 * expression - which is exactly what the splice bargain forbids. So the distinction moved to the
 * attribute NAME, which is ours to match and needs no parsing: Attr(from) evaluates, Attr(with)
 * applies"
 *
 * NOTE(#from/not-total): V[Attr(from).optional], "#[from] is CONVENIENCE and is not required to be
 * sufficient. Extraction that needs real logic - correlating two sources, conditioning on something
 * the path cannot see - writes extract_from by hand, which stays fully available and is what every
 * extractor in this crate does today. The attribute exists so the easy majority stops being written
 * out longhand, not so the hard minority becomes expressible in an attribute. Resist growing it a
 * vocabulary for the latter; that is the darling failure mode in a different costume"
 *
 * Query(#from/native-and-custom): Q[Ty(Source).T(custom) ??], "RESTATED against the current shape -
 * the old wording said 'generic over I: Visitable', and `I` no longer exists: the source became an
 * ASSOCIATED TYPE in NOTE(#pipeline/source-is-associated). The substance is unchanged. A syn node
 * works as a Ty(Source) today. A CUSTOM grammar node needs Tr(Visitable) generalised over the
 * visitor family - PROVEN to work (one walk() drove a syn node and a grammar node in the same
 * shape) but not landed, because nothing has needed it.
 *
 * Note that this is the SAME question @group(#multi-source) asks from the other side: 'several
 * sources for one extraction' and 'our own nodes as sources' are both answered by what Ty(Source)
 * is allowed to be. Decide them together"
 */
