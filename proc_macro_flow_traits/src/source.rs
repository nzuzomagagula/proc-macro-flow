// @review [ ]
//! Where an extraction came from.
//!
//! Every diagnostic this framework emits has to point at something the user wrote, so carrying the
//! origin cannot be left to whoever remembers. `Sourced::source` is a REQUIRED method: an extractor
//! that does not carry its source does not compile.
//!
//! NOTE(#source-not-span): V[Ty(Source) != T(Span)], "The source is the NODE, not a Span, and that
//! is not a preference. VERIFIED against syn 3.0.4: proc_macro2::Span::join is nightly-only and
//! returns None on stable, so syn::spanned::Spanned::span() falls back to the FIRST TOKEN alone -
//! a stored Span would underline `AttributeKind` and not `AttributeKind::MetaList`. syn::Error::
//! new_spanned avoids this by taking start and end from the first and last token, which it can
//! only do if it is handed the node. So: hold the node, derive the span at render time"
//!
//! NOTE(#no-ancestry): V[Tr(Sourced).!has(F(ancestors))], "A node carries its OWN source and
//! nothing above it. The tree already encodes nesting, so storing a parent chain per node would be
//! a second copy of a fact the structure already holds - and two copies can disagree. The vector of
//! spans a nested diagnostic wants is built by the render walk on its way down, not stored"

use quote::ToTokens;

pub trait Sourced<'ast> {
    /// The node this was extracted from. For most extractors this is exactly the `I` of
    /// `Extractor<'ast, I>`.
    type Source: ToTokens + ?Sized;

    /// Required, and that is the whole point of the trait.
    fn source(&self) -> &'ast Self::Source;

    /// Extra sources for a node folded from several places - two attributes contributing to one
    /// grammar node, say. Defaulted so the overwhelmingly common single-source case pays nothing.
    fn additional(&self) -> &[&'ast Self::Source] {
        &[]
    }
}
