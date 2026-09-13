// @review [ ]
//! Deferred items: tokens that have been carried but not yet read.
//!
//! The pipeline needs to move an item from one stage to the next without committing to *when* it
//! gets parsed - a selector may never need parsing at all (it is re-emitted and resolved by rustc),
//! while a spine node has to be read before anything can branch on it. What must not happen is a
//! runtime flag that every consumer has to remember to check.
//!
//! So the state is a TYPESTATE. `Unresolved` and `Resolved` are distinct types, the transition
//! consumes the old one, and `Resolved` has no `resolve` - double resolution is a type error rather
//! than a silent no-op, and reading an unparsed item does not compile.
//!
//! NOTE(#no-idempotence): V[S(Resolved).!has(F(resolve))], "There is deliberately no idempotent
//! resolve(). An idempotent operation is one whose SECOND call has to be handled; here the second
//! call cannot be written, because `resolve` consumes an `Unresolved` and hands back a `Resolved`
//! that has no such method. The ordering bug is not guarded against, it is unrepresentable"

use core::marker::PhantomData;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse::Parse;

/// Tokens whose meaning has not been read yet.
///
/// `T` says what they are *expected* to resolve to and is otherwise unused, so it is held as
/// `PhantomData<fn() -> T>` - covariant, and imposing neither `Send`/`Sync` nor drop obligations
/// that a real `T` would.
pub struct Unresolved<'ast, T> {
    tokens: &'ast TokenStream,
    _target: PhantomData<fn() -> T>,
}

/// An item that has been read, with the tokens it was read from kept alongside.
///
/// Keeping the tokens is what lets a resolved item still be spliced verbatim and still point at
/// what the user wrote; resolving never costs you the source.
pub struct Resolved<'ast, T> {
    value: T,
    tokens: &'ast TokenStream,
}

/// The surface common to both states.
///
/// This is what the `Stage::Item` bound is for: without it, code generic over a stage would hold
/// an item it could not do anything with.
pub trait Deferred<'ast> {
    fn tokens(&self) -> &'ast TokenStream;
}

impl<'ast, T> Unresolved<'ast, T> {
    pub fn new(tokens: &'ast TokenStream) -> Self {
        Self { tokens, _target: PhantomData }
    }

    /// Resolve through a caller-supplied reader.
    ///
    /// A grammar type is read by its shape trait, not by `syn::Parse`, so this must not assume
    /// syn's parser is the only one that exists.
    pub fn resolve_with<E>(
        self,
        read: impl FnOnce(&'ast TokenStream) -> Result<T, E>,
    ) -> Result<Resolved<'ast, T>, E> {
        read(self.tokens).map(|value| Resolved { value, tokens: self.tokens })
    }
}

impl<'ast, T: Parse> Unresolved<'ast, T> {
    /// Resolve through `syn`.
    ///
    /// `syn::parse2` takes the stream by value, so this clones - unavoidable given that signature,
    /// and paid once per item at expansion time.
    pub fn resolve(self) -> Result<Resolved<'ast, T>, syn::Error> {
        self.resolve_with(|tokens| syn::parse2::<T>(tokens.clone()))
    }
}

impl<'ast, T> Resolved<'ast, T> {
    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

impl<'ast, T> Deferred<'ast> for Unresolved<'ast, T> {
    fn tokens(&self) -> &'ast TokenStream {
        self.tokens
    }
}

impl<'ast, T> Deferred<'ast> for Resolved<'ast, T> {
    fn tokens(&self) -> &'ast TokenStream {
        self.tokens
    }
}

/// Splicing is uniform across states: both emit the tokens they came from, bearing the spans they
/// came in with. A generator never has to know, or ask, whether an item was resolved.
impl<T> ToTokens for Unresolved<'_, T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.tokens.to_tokens(tokens);
    }
}

impl<T> ToTokens for Resolved<'_, T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.tokens.to_tokens(tokens);
    }
}

/// Which state every deferred item in a container is in.
///
/// Stage-level rather than item-level: a container takes ONE parameter however many deferred
/// fields it has, and transitions wholesale. The cost is that a container cannot hold a mix, which
/// is the point - "half parsed" is not a state anything downstream should have to consider.
pub trait Stage {
    type Item<'ast, T>: Deferred<'ast> + ToTokens;
}

/// Marker: nothing in this container has been read yet.
pub enum Raw {}

/// Marker: everything in this container has been read.
pub enum Parsed {}

impl Stage for Raw {
    type Item<'ast, T> = Unresolved<'ast, T>;
}

impl Stage for Parsed {
    type Item<'ast, T> = Resolved<'ast, T>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::Type;

    // A container exercising the Stage GAT exactly as a real grammar node would: one parameter,
    // several deferred fields, projected in field position.
    struct Node<'ast, S: Stage> {
        shape: S::Item<'ast, Type>,
        alias: S::Item<'ast, syn::Ident>,
    }

    // Generic over the stage: only compiles because Stage::Item is bounded by Deferred + ToTokens.
    fn spliced<S: Stage>(node: &Node<'_, S>) -> String {
        let (shape, alias) = (&node.shape, &node.alias);
        quote!(#shape #alias).to_string()
    }

    #[test]
    fn resolve_transitions_and_keeps_the_tokens() {
        let tokens = quote!(AttributeKind::MetaList);
        let unresolved = Unresolved::<Type>::new(&tokens);

        assert_eq!(unresolved.tokens().to_string(), tokens.to_string());

        let resolved = unresolved.resolve().expect("a path is a type");

        // The source survives the transition - this is what keeps a resolved item spliceable and
        // still able to point at what the user wrote.
        assert_eq!(resolved.tokens().to_string(), tokens.to_string());
        assert!(matches!(resolved.value(), Type::Path(_)));
    }

    #[test]
    fn splicing_is_identical_either_side_of_the_transition() {
        let shape = quote!(AttributeKind::MetaList);
        let alias = quote!(label);

        let raw = Node::<Raw> {
            shape: Unresolved::new(&shape),
            alias: Unresolved::new(&alias),
        };
        let before = spliced(&raw);

        let parsed = Node::<Parsed> {
            shape: raw.shape.resolve().unwrap(),
            alias: raw.alias.resolve().unwrap(),
        };

        // A generator never has to ask which state it is holding.
        assert_eq!(before, spliced(&parsed));
    }

    #[test]
    fn resolving_garbage_is_an_error_that_can_be_emitted() {
        let tokens = quote!(!!);
        let error = Unresolved::<Type>::new(&tokens)
            .resolve()
            .err()
            .expect("`!!` is not a type");

        assert!(!error.to_compile_error().is_empty());
    }

    #[test]
    fn resolve_with_does_not_require_parse() {
        // The escape hatch for grammar types, which are read by their shape trait rather than by
        // syn::Parse. `u8` has no Parse impl, so this only compiles via resolve_with.
        let tokens = quote!(anything);
        let resolved = Unresolved::<u8>::new(&tokens)
            .resolve_with(|_| Ok::<_, ()>(7u8))
            .expect("the supplied reader succeeds");

        assert_eq!(*resolved.value(), 7);
        assert_eq!(resolved.tokens().to_string(), tokens.to_string());
    }
}
