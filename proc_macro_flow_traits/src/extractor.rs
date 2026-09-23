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

    /// OURS. A framework failure - malformed tokens we assembled, a contract we broke.
    ///
    /// Never re-spanned onto the author's syntax: blaming them for our bug is worse than a vague
    /// error, because they go looking at correct code. See NOTE(#reason/fault-is-declared).
    Internal(syn::Error),

    /// A real syn failure attributable to what the AUTHOR wrote.
    ///
    /// Almost always their tokens landing somewhere they do not fit - which is common precisely
    /// because this design SPLICES their input rather than interpreting it (ID(no-parse)).
    Syntax(syn::Error),

    /// A grammar author's own reason, already worded.
    ///
    /// Stays a `String` where the two above hold an error, and that asymmetry is the point: for
    /// these the framework keeps control of span and position, which is what ID(syntax/diagnostics)
    /// exists to protect. For ours there is nobody to take control from - the error arrived from
    /// `parse2` already well formed.
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

/// A grammar author's own reason, rendered BY THE FRAMEWORK.
///
/// NOTE(#reason/author-writes-meaning-not-errors): V[Tr(AuthorReason).M(message) && !Tr(AuthorReason).R(Error)],
/// "An author supplies MEANING - what went wrong, in their words - and the framework supplies
/// everything else: the span, the position in the tree, the rendering, the accumulation. Handing
/// them `E(ReasonKind)::Custom(String)` and asking them to build an error was the framework
/// abdicating the half it is actually good at.
///
/// This is also how an author gets a REASON TREE without the framework growing a generic. They
/// declare their own enum, match on it exhaustively in their own code, give it a F(message), and it
/// converts at the boundary - the same shape M(vocabulary) uses for names. The alternative,
/// `Reason<K>` generic over an author's kind, would thread a parameter through Ty(Extraction),
/// Ty(Extracted), Tr(Diagnose) and Tr(Pipeline) to serve an escape hatch.
///
/// F(span) has a DEFAULT of None, which is the convenience that matters: an author who has nothing
/// finer than the node says nothing, and ID(reason/span-not-node)'s fallback does the rest"
pub trait AuthorReason {
    /// What went wrong, in the author's words.
    fn message(&self) -> String;

    /// A finer span than the node this reason will sit on, when the author has one.
    fn span(&self) -> Option<Span> {
        None
    }
}

impl Reason {
    /// A complaint about the node this reason will sit on.
    pub fn new(kind: ReasonKind) -> Self {
        Self { kind, span: None }
    }

    /// An author's own reason, converted.
    ///
    /// The way in for anything Tr(AuthorReason) describes - and the reason `ReasonKind::Custom`
    /// should rarely be written by hand. Not a `From` impl: a blanket
    /// `impl<T: AuthorReason> From<T> for Reason` collides with core's reflexive `From<T> for T`,
    /// because coherence cannot rule out `Reason: AuthorReason`.
    pub fn custom(reason: impl AuthorReason) -> Self {
        Self {
            kind: ReasonKind::Custom(reason.message()),
            span: reason.span(),
        }
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
    /// NOTE(#reason/error-is-carried-not-rebuilt): V[F(to_error).returns(held)], "Where the kind
    /// HOLDS a syn::Error, it is returned as-is rather than rebuilt from its message. Rebuilding
    /// flattens: `syn::Error::combine` keeps each sub-error's own span, and
    /// `Error::new_spanned(node, message)` would collapse all of them into one message at one
    /// span. Returning it preserves every sub-error and every span all the way to
    /// `to_compile_error`, which is the whole reason the kind carries an error at all.
    ///
    /// SPAN PRECEDENCE, so it is written down rather than discovered: a Reason's OWN span wins when
    /// it has one, because that is the finer pointer ID(reason/span-not-node) describes - a reason
    /// that points at one token is exact. The held error's spans survive inside it either way"
    pub fn to_error(&self, node: &impl ToTokens, message: impl core::fmt::Display) -> syn::Error {
        match &self.kind {
            ReasonKind::Internal(error) | ReasonKind::Syntax(error) => match self.span {
                Some(span) => syn::Error::new(span, error),
                None => error.clone(),
            },
            _ => match self.span {
                Some(span) => syn::Error::new(span, message),
                None => syn::Error::new_spanned(node, message),
            },
        }
    }

    /// Whether this is OUR failure rather than the author's.
    ///
    /// NOTE(#reason/fault-is-declared): V[M(is_internal).!heuristic], "Fault is DECLARED at the
    /// raise site, never inferred. `parse2` cannot tell 'the author's token did not fit here' from
    /// 'we assembled this wrong' - both are just a parse failure - so a leaf that splices author
    /// input raises E(Syntax) and one assembling fixed tokens raises E(Internal). Guessing would be
    /// worse than asking, because the cost of guessing wrong is pointing an author at their own
    /// correct code"
    pub fn is_internal(&self) -> bool {
        matches!(self.kind, ReasonKind::Internal(_))
    }

    /// Adopt `span` only if this reason has nothing finer of its own.
    ///
    /// ID(reason/span-not-node)'s fallback rule, promoted from prose into a method. It is what lets
    /// a parent say WHERE a child's failure belongs without overwriting a child that already knew.
    pub fn or_span(mut self, span: Span) -> Self {
        self.span = self.span.or(Some(span));
        self
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

// NOTE(#pipeline/validity-scope): F(validate) is a SURFACE check - is this node one of ours, and what
// does it narrow to. It interprets no tokens; that is the reader's job.
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

    // NOTE(#pipeline/validity-error): a validity failure is a E(Reason) - a span and a cause - and never a
    // per-type error. A proc macro only ever EMITS an error, so a taxonomy buys nothing and cannot
    // combine with a sibling's.
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

pub trait Extractor<'ast>: Sized + Validate<'ast> {
    /// What the processor receives.
    ///
    /// NOTE(#extractor/output-bound): Ty(Output) carries NO bound. An extractor does not know which
    /// processor will consume it; the agreement is declared from Tr(Pipeline), where the stages are named
    /// together.
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
