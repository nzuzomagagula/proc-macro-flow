// @review [ ]

use proc_macro2::Ident;
use proc_macro_flow_traits::{
    extractor::{Extraction, Reason, ReasonKind},
    resolution::{Parsed, Raw, Stage, Unresolved},
    source::Sourced,
};
use syn::{Attribute, Type};

use crate::traits::{Validate, extractor::Extractor};

// DEPRECATED(#syntax/attribute-kind):D[E(AttributeKind)], "Deleted. It existed so extract_from
// could hand-match which shape a #[shape(..)] argument named, and that matching is the thing the
// design removes: the argument is re-emitted into a type position where RUSTC resolves it, which
// is why the deferred target below is syn::Type and not an enum of ours. VERIFIED that this pays:
// a typo gets rustc's own did-you-mean WITH a machine-applicable fix, a shape the field type
// cannot read gets 'but trait FromMeta<MetaList> is implemented for it', and a non-shape gets the
// full candidate list - none of which a hand-written matcher here could produce. The half-written
// const ATTRIBUTE_KIND went with it: canonical paths are rustc's business now, not a table's"

pub struct SyntaxExtraction<'ast, S: Stage> {
    fields: Vec<Extraction<SyntaxFieldExtraction<'ast, S>>>,
    alias: SyntaxAttributeExtraction<'ast>,
}

pub struct SyntaxAttributeExtraction<'ast> {
    alias: &'ast Ident,
}

pub struct SyntaxFieldExtraction<'ast, S: Stage> {
    attributes: Vec<Extraction<SyntaxFieldAttributeExtraction<'ast, S>>>,
    field_ident: &'ast Ident,
    field_ty: &'ast Type,
}

/// One helper attribute on a grammar field.
///
/// The attribute is held separately from the payload so the node has a source in every variant -
/// `Sourced::source` is required, and an enum whose arms each carried their own span would have
/// had nothing to give it for `Alias`.
pub struct SyntaxFieldAttributeExtraction<'ast, S: Stage> {
    attribute: &'ast Attribute,
    kind: SyntaxFieldAttributeKind<'ast, S>,
}

pub enum SyntaxFieldAttributeKind<'ast, S: Stage> {
    /// `#[shape(..)]`'s argument, deferred.
    ///
    /// The target is `syn::Type` because that is the POSITION the generator splices it into, not
    /// because anything here intends to read it. On the compiler-checked path this item is never
    /// resolved at all - it is re-emitted verbatim and rustc resolves the path. Resolving it stays
    /// available for a stage that genuinely needs to inspect the selection.
    Shape(S::Item<'ast, Type>),
    /// `#[alias(..)]`'s argument. An alias INTRODUCES a name rather than referring to one, so
    /// there is nothing for rustc to resolve and it is read eagerly as a plain ident.
    Alias(&'ast Ident),
}

impl<'ast, S: Stage> Sourced<'ast> for SyntaxFieldAttributeExtraction<'ast, S> {
    type Source = Attribute;

    fn source(&self) -> &'ast Attribute {
        self.attribute
    }
}

/// Extraction always lands in `Raw`, never in a generic `S`.
///
/// That is forced rather than chosen: only `Raw` can build an item out of loose tokens, so a
/// `Stage`-generic `extract_from` could not construct its own payload. Moving to `Parsed` is
/// `resolve`'s job below, which is exactly the separation the typestate exists to draw.
impl<'ast> Extractor<'ast, &'ast Attribute> for SyntaxFieldAttributeExtraction<'ast, Raw> {
    fn extract_from(node: &'ast Attribute) -> Extraction<Self> {
        let Ok(attribute) = Self::validate(node) else {
            return Extraction::failed(Reason::new(ReasonKind::UnknownKey, node));
        };

        if attribute.path().is_ident("shape") {
            return match attribute.meta.require_list() {
                Ok(list) => Extraction::value(Self {
                    attribute,
                    kind: SyntaxFieldAttributeKind::Shape(Unresolved::new(&list.tokens)),
                }),
                Err(_) => Extraction::failed(Reason::new(ReasonKind::WrongShape, node)),
            };
        }

        if attribute.path().is_ident("alias") {
            return match attribute.parse_args::<Ident>() {
                // Not deferred: see SyntaxFieldAttributeKind::Alias.
                Ok(_) => Extraction::failed(Reason::new(ReasonKind::Custom(
                    "alias parsing needs an 'ast-lived Ident; blocked on #syntax/alias-storage"
                        .to_owned(),
                ), node)),
                Err(_) => Extraction::failed(Reason::new(ReasonKind::WrongShape, node)),
            };
        }

        Extraction::failed(Reason::new(ReasonKind::UnknownKey, node))
    }
}

impl<'ast> SyntaxFieldAttributeExtraction<'ast, Raw> {
    /// The typestate transition.
    ///
    /// Note what cannot be written: there is no `resolve` on the `Parsed` form, so resolving twice
    /// is a type error rather than a silent no-op, and nothing downstream has to check a flag to
    /// know which state it is holding.
    pub fn resolve(self) -> Extraction<SyntaxFieldAttributeExtraction<'ast, Parsed>> {
        let attribute = self.attribute;
        let kind = match self.kind {
            SyntaxFieldAttributeKind::Shape(shape) => match shape.resolve() {
                Ok(resolved) => SyntaxFieldAttributeKind::Shape(resolved),
                Err(_) => {
                    return Extraction::failed(Reason::new(ReasonKind::WrongShape, attribute));
                }
            },
            SyntaxFieldAttributeKind::Alias(alias) => SyntaxFieldAttributeKind::Alias(alias),
        };

        Extraction::value(SyntaxFieldAttributeExtraction { attribute, kind })
    }
}

impl<'ast, S: Stage> Validate<'ast, &'ast Attribute> for SyntaxFieldAttributeExtraction<'ast, S> {
    type ValidityError = SyntaxFieldAttributeError;

    type Valid = &'ast Attribute;

    // TODO[ ](#syntax/attribute-validate):U[F(validate)], "Narrows nothing yet. What belongs here
    // is the head check - `shape` or `alias` and nothing else - which extract_from currently does
    // inline. Moving it needs Valid to become a narrowed type rather than the attribute back
    // again, and that is ID(syntax/extraction)'s shape"
    fn validate(input: &'ast Attribute) -> Result<Self::Valid, Self::ValidityError> {
        Ok(input)
    }
}

pub struct SyntaxFieldAttributeError;
