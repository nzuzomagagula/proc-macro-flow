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
        Self { value: Some(value), reasons: Vec::new() }
    }

    pub fn failed(reason: Reason) -> Self {
        Self { value: None, reasons: vec![reason] }
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
        Self { value: None, reasons: Vec::new() }
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
        Self { kind, span: Some(token.span()) }
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

        assert!(!precise.to_error(&node, "unknown key").to_compile_error().is_empty());
        assert!(!coarse.to_error(&node, "wrong shape").to_compile_error().is_empty());
    }

    #[test]
    fn a_reason_cannot_render_itself_without_the_chain() {
        // `to_error` takes the fallback node as an argument on purpose: position belongs to where a
        // reason SITS in the tree, not to the reason. A reason with no span and no node has nothing
        // to point at, and the signature makes that unrepresentable rather than merely discouraged.
        let node = quote!(colour(ColourSetting::Red));
        let reason = Reason::new(ReasonKind::Missing);

        assert!(!reason.to_error(&node, "missing required key").to_compile_error().is_empty());
    }

    #[test]
    fn absorb_takes_every_reason_even_from_a_child_that_produced_nothing() {
        let mut parent = Extraction::value("parent");

        let good: Extraction<u8> = Extraction::value(1)
            .with_reason(Reason::at(ReasonKind::UnknownKey, &quote!(colur)));
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
