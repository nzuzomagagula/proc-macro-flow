// @review [x]

use proc_macro2::Ident;
use proc_macro_flow_traits::{
    extractor::{Extracted, Extraction, Reason, ReasonKind},
    resolution::{Deferred, Parsed, Raw, Stage, Unresolved},
};
use syn::{Attribute, Type};

use proc_macro_flow_traits::extractor::{Extractor, Validate};
use proc_macro_flow_traits::render::Diagnose;

// NOTE(#heads-are-rustcs):V[F(extract_from).!emits(E(ReasonKind).V(UnknownKey))], "An attribute
// HEAD we do not recognise is never our complaint, and this stage must stay silent about one.
// VERIFIED: a derive registers its helpers with attributes(shape, alias), and rustc rejects any
// other head BEFORE the macro runs - `#[shpae(..)]` gets 'cannot find attribute `shpae` in this
// scope' plus TWO machine-applicable fixes, one of which names the derive that accepts `shape`.
// Better than anything we could write, and free. So a head that does reach us and is not ours
// belongs to ANOTHER macro - #[doc], #[cfg], #[serde] - and reporting it would be actively wrong.
// A doc comment is an attribute, so the earlier UnknownKey here fired on every documented field.
// The residue for us is KEYS INSIDE the delimiters, where rustc resolves nothing: `colur(Red)` is
// still ours to catch, because no compiler pass can see it"


proc_macro_flow_traits::vocabulary! {
    /// The helper attributes this stage registers, and the only heads it owns.
    ///
    pub enum SyntaxHelper {
        Shape = "shape",
        Alias = "alias",
    }
}

/// One helper attribute on a grammar field.
///
/// NOTE(#syntax/resolution-is-deferred-not-dead): V[M(resolve).tested && !M(resolve).on_macro_path],
/// TODO[ ](#syntax/attribute-duplicates-source): `attribute` is read in ONE place - `resolve`,
/// moving it into the Parsed value - and otherwise duplicates Extracted::source(). It survives only
/// because `resolve` returns a bare Extraction with no Extracted wrapper to ask. Making resolution
/// preserve the wrapper would remove the duplication ID(extractor/two-questions) removed
/// everywhere else. That is the resolution stage's business, not this one's."
pub(crate) struct SyntaxFieldAttributeExtraction<'ast, S: Stage> {
    #[allow(dead_code, reason = "read by resolve; see #syntax/resolution-is-deferred-not-dead")]
    attribute: &'ast Attribute,
    #[allow(dead_code, reason = "read by kind(); see #syntax/resolution-is-deferred-not-dead")]
    kind: SyntaxFieldAttributeKind<'ast, S>,
}

pub(crate) enum SyntaxFieldAttributeKind<'ast, S: Stage> {
    /// `#[shape(..)]`'s argument, deferred.
    ///
    /// The target is `syn::Type` because that is the POSITION the generator splices it into, not
    /// because anything here intends to read it. On the compiler-checked path this item is never
    /// resolved at all - it is re-emitted verbatim and rustc resolves the path. Resolving it stays
    /// available for a stage that genuinely needs to inspect the selection.
    Shape(S::Item<'ast, Type>),
    /// `#[alias(..)]`'s argument, also deferred.
    ///
    /// An alias INTRODUCES a name rather than referring to one, so unlike a shape there is nothing
    /// for RUSTC to resolve - it is never spliced into a checked position. It is still deferred,
    /// because "who resolves it" and "when" are different questions: this one is read by us, at
    /// whichever stage asks. Deferring it is also what lets it be owned once resolved, which a
    /// `&'ast Ident` could never be - an ident parsed out of attribute tokens is not in the AST.
    Alias(S::Item<'ast, Ident>),
}

impl<'ast, S: Stage> SyntaxFieldAttributeExtraction<'ast, S> {
    #[allow(dead_code, reason = "see NOTE(#syntax/resolution-is-deferred-not-dead)")]
    pub(crate) fn kind(&self) -> &SyntaxFieldAttributeKind<'ast, S> {
        &self.kind
    }
}

/// Extraction always lands in `Raw`, never in a generic `S`.
///
/// That is forced rather than chosen: only `Raw` can build an item out of loose tokens, so a
/// `Stage`-generic `extract_from` could not construct its own payload. Moving to `Parsed` is
/// `resolve`'s job below, which is exactly the separation the typestate exists to draw.
impl<'ast> Extractor<'ast> for SyntaxFieldAttributeExtraction<'ast, Raw> {
    type Output = Extracted<Self, &'ast Attribute>;

    fn extract_from(node: &'ast Attribute) -> Self::Output {
        fn read<'ast>(
            node: &'ast Attribute,
        ) -> Extraction<SyntaxFieldAttributeExtraction<'ast, Raw>> {
            // Not ours: no value, and no complaint either. See ID(heads-are-rustcs).
            let Ok((attribute, helper)) = SyntaxFieldAttributeExtraction::<Raw>::validate(node)
            else {
                return Extraction::default();
            };

            let Ok(list) = attribute.meta.require_list() else {
                return Extraction::failed(Reason::new(ReasonKind::WrongShape));
            };

            // Both arms only CARRY the argument tokens - neither reads them. What `shape` names is
            // resolved by rustc at the splice site; what `alias` names is read later, by whichever
            // stage asks. See NOTE(#no-parse). They cannot share a constructor despite looking
            // alike: Shape defers a Type and Alias an Ident, so the payloads differ in type.
            //
            // Exhaustive over SyntaxHelper, and deliberately so - a new helper stops compiling here
            // rather than silently doing nothing.
            let kind = match helper {
                SyntaxHelper::Shape => {
                    SyntaxFieldAttributeKind::Shape(Unresolved::new(&list.tokens))
                }
                SyntaxHelper::Alias => {
                    SyntaxFieldAttributeKind::Alias(Unresolved::new(&list.tokens))
                }
            };

            Extraction::value(SyntaxFieldAttributeExtraction { attribute, kind })
        }

        Extracted::new(read(node), node)
    }
}

impl<'ast> SyntaxFieldAttributeExtraction<'ast, Raw> {
    /// The typestate transition.
    ///
    /// Note what cannot be written: there is no `resolve` on the `Parsed` form, so resolving twice
    /// is a type error rather than a silent no-op, and nothing downstream has to check a flag to
    /// know which state it is holding.
    #[allow(dead_code, reason = "see NOTE(#syntax/resolution-is-deferred-not-dead)")]
    pub(crate) fn resolve(self) -> Extraction<SyntaxFieldAttributeExtraction<'ast, Parsed>> {
        let attribute = self.attribute;

        // `resolve` hands back a bare Extraction with no Extracted around it, so there is no chain
        // to fall back on - these reasons have to carry their own span. The argument tokens are
        // what failed to resolve, so they are what gets underlined, not the whole attribute.
        let kind = match self.kind {
            SyntaxFieldAttributeKind::Shape(shape) => {
                let tokens = shape.tokens();
                match shape.resolve() {
                    Ok(resolved) => SyntaxFieldAttributeKind::Shape(resolved),
                    Err(_) => {
                        return Extraction::failed(Reason::at(ReasonKind::WrongShape, tokens));
                    }
                }
            }
            SyntaxFieldAttributeKind::Alias(alias) => {
                let tokens = alias.tokens();
                match alias.resolve() {
                    Ok(resolved) => SyntaxFieldAttributeKind::Alias(resolved),
                    Err(_) => {
                        return Extraction::failed(Reason::at(ReasonKind::WrongShape, tokens));
                    }
                }
            }
        };

        Extraction::value(SyntaxFieldAttributeExtraction { attribute, kind })
    }
}

impl<'ast, S: Stage> Validate<'ast> for SyntaxFieldAttributeExtraction<'ast, S> {
    type Source = &'ast Attribute;
    /// A genuine narrowing, not the input handed back: the head is now RESOLVED to the vocabulary
    /// entry it names, so no later stage repeats the comparison. This is what `Valid` is for.
    type Valid = (&'ast Attribute, SyntaxHelper);

    // Surface-level and nothing more, which is exactly ID(pipeline/validity-scope)'s remit: is
    // this attribute one of ours? No token is interpreted to answer it.
    fn validate(input: &'ast Attribute) -> Result<Self::Valid, Reason> {
        SyntaxHelper::try_from(input.path())
            .map(|helper| (input, helper))
            // OFFERED and, by this extractor, DELIBERATELY DROPPED - see
            // NOTE(#validate/reason-is-offered-not-imposed) and ID(heads-are-rustcs). The reason is
            // constructed so the signature is honest about what failed; extract_from below throws
            // it away because 'not one of ours' is not a complaint.
            .map_err(|_| Reason::at(ReasonKind::UnknownKey, input.path()))
    }
}

impl<'ast, S: Stage> Diagnose for SyntaxFieldAttributeExtraction<'ast, S> {
    /// A LEAF, and that is a statement about the design rather than a stub.
    ///
    /// This node's payload is `kind`, which holds CARRIED tokens - an `Unresolved<Type>` or
    /// `Unresolved<Ident>` - not child extractions. Nothing below it has reasons of its own,
    /// because nothing below it has been read: that is ID(no-parse). Its own reasons are rendered
    /// by the `Extracted` around it, per NOTE(#render/who-renders).
    ///
    /// It stops being a leaf if and when a nested grammar node becomes an extraction in its own
    /// right, which is ID(syntax/extraction)'s business.
    fn diagnose(&self, _: &mut Vec<syn::Error>) {}
}
