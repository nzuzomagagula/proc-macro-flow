// @review [ ]
//! `Extraction<T>` - a value, the complaints attached to it, or both.
//!
//! Answers ID(cleanup): the shape is `{ value: Option<T>, reasons: Vec<Reason> }` and NOT a
//! typestate, because a node can extract perfectly well AND carry a complaint of its own - an
//! unknown key is the parent failing to consume its input while the value stays good. Two states
//! could not say that. Contrast ID(no-idempotence) in `resolution`, where resolved/unresolved has
//! no second axis and therefore IS a typestate.
//!
//! NOTE(#no-result): V[F(extract_from).R(Extraction) != R(Result)], "extract_from returns
//! Extraction<Self>, never Result. With no `?` there is no early return, and with no early return
//! there is no way to drop a sibling or lose a position on the way out. The enforcement is
//! structural rather than a convention somebody has to hold to"

use proc_macro2::TokenStream;
use quote::ToTokens;

pub struct Extraction<T> {
    pub value: Option<T>,
    pub reasons: Vec<Reason>,
}

/// Why a node is unhappy, and what to point at.
///
/// The source is an OWNED snapshot rather than a borrow: a reason outlives the node it was recorded
/// on and gets rendered in one final pass, long after the walk that produced it. This is the error
/// path, so the clone costs nothing anybody will measure - nodes themselves still hold cheap
/// `&'ast` sources through `Sourced`.
pub struct Reason {
    pub kind: ReasonKind,
    source: TokenStream,
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
    pub fn new(kind: ReasonKind, source: &impl ToTokens) -> Self {
        Self { kind, source: source.to_token_stream() }
    }

    /// The recorded tokens, for a renderer that wants to span something itself.
    pub fn source(&self) -> &TokenStream {
        &self.source
    }

    // TODO[ ](#reason/message):U[F(to_error)], "The message is a parameter today because deciding
    // it belongs to ID(diagnostics) crossed with ID(node-table), neither of which exists. What is
    // already settled is that it is rendered LATE, from kind plus tree position - baking a string
    // at record time is what would put it out of an author's reach"
    pub fn to_error(&self, message: impl core::fmt::Display) -> syn::Error {
        syn::Error::new_spanned(&self.source, message)
    }
}
