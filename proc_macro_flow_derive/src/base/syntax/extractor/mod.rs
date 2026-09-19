// @review [x]
// UNWIRED(#syntax/not-driven): V[N(extractor).tested && !N(extractor).reachable], "Everything below
// is exercised by the tests in base/syntax/worked.rs and by nothing on the macro's actual path -
// lib.rs::extractor drives StructExtraction and never reaches the syntax stage. The allow is here
// so a clean build stays meaningful rather than carrying nine standing warnings; it comes OFF the
// moment ID(attribute/generic-grammar) connects the stages, and if anything in here is still
// unreachable then, it is genuinely dead and should go"
#![allow(dead_code)]

use proc_macro_flow_traits::{
    extractor::{Extracted, Extraction, Reason, ReasonKind},
    resolution::{Deferred, Parsed, Raw, Stage, Unresolved},
};
use proc_macro2::Ident;
use syn::{Attribute, Type};

use proc_macro_flow_traits::extractor::{Extractor, Validate};

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
// DEPRECATED(#syntax/attribute-kind):D[E(AttributeKind)], "Deleted. It existed so extract_from
// could hand-match which shape a #[shape(..)] argument named, and that matching is the thing the
// design removes: the argument is re-emitted into a type position where RUSTC resolves it, which
// is why the deferred target below is syn::Type and not an enum of ours. VERIFIED that this pays:
// a typo gets rustc's own did-you-mean WITH a machine-applicable fix, a shape the field type
// cannot read gets 'but trait FromMeta<MetaList> is implemented for it', and a non-shape gets the
// full candidate list - none of which a hand-written matcher here could produce. The half-written
// const ATTRIBUTE_KIND went with it: canonical paths are rustc's business now, not a table's"

// DEPRECATED(#syntax/parent-nodes):D[S(SyntaxExtraction)] && D[S(SyntaxAttributeExtraction)] && D[S(SyntaxFieldExtraction)], "Deleted as stale scaffolding, not because the stage does not need parent nodes. Three things were wrong with them and none was superficial: they held Vec<Extraction<..>> where every other extractor now yields Extracted<.., &'ast Node>, so they predate the source-on-the-output decision; SyntaxAttributeExtraction held `alias: &'ast Ident`, which is the exact bug already fixed on the FIELD alias - an ident parsed out of attribute tokens is not in the AST and cannot be borrowed from it, so it has to be deferred as S::Item<'ast, Ident>; and none had an Extractor impl, so nothing constrained them to stay honest. Re-declare them when the stage is actually wired, from the working SyntaxFieldAttributeExtraction below outwards, rather than carrying a wrong shape forward"

proc_macro_flow_traits::vocabulary! {
    /// The helper attributes this stage registers, and the only heads it owns.
    ///
    /// Answer(#attribute/helpers):A[ID(attribute) ==? M(vocabulary)], "The query was right that a
    /// string comparison has to happen SOMEWHERE - tokens are text, and a head is matched by its
    /// spelling because we invented that spelling. What was wrong was that it had to happen HERE,
    /// by hand, at every site. A vocabulary confines it to one generated From/TryFrom and makes
    /// everything downstream typed. The property that pays for it: the match in extract_from is now
    /// EXHAUSTIVE over this enum, so adding a helper is a compile error at every site that handles
    /// one. With `is_ident(\"..\")` strings, adding a helper silently did nothing anywhere"
    ///
    /// Note what this is NOT for: a value path like `ColourSetting::Red`. See
    /// ID(vocabulary/only-what-we-own) - values are rustc's to resolve, and matching them by
    /// spelling cannot see through a qualified path or a renamed import.
    pub enum SyntaxHelper {
        Shape = "shape",
        Alias = "alias",
    }
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
    //TODO[ ](#review):Q[this, "Why a Type, not the variant expression? If the generator intends on using it, then it would parse as like the variant path?"]
    // Answer(#review/shape-position):A[ID(review) ==? T(Type)], "Because Type is the POSITION, not a guess at the content. The generator splices these tokens into `type Selected = #tokens;` - a type position - so Type is the widest category that site accepts, and the rule is to validate only that the tokens are EMITTABLE, never what they mean. Narrowing to Path would have THIS crate reject #[shape(Vec<u8>)] with a syn parse error of our own; letting it through gets rustc's 'the trait bound `Vec<u8>: Shape` is not satisfied' PLUS the full list of types that do implement Shape - strictly the better message, and free. Nothing is lost by the wider type: syn::Type::Path recovers the path whenever it IS one, so a stage that wants the variant path just matches Type::Path. And the premise needs correcting - the generator never 'uses' this. It re-emits it and rustc resolves it, which is the whole reason this arm is deferred rather than parsed"
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
    pub fn kind(&self) -> &SyntaxFieldAttributeKind<'ast, S> {
        &self.kind
    }
}

/// Extraction always lands in `Raw`, never in a generic `S`.
///
/// That is forced rather than chosen: only `Raw` can build an item out of loose tokens, so a
/// `Stage`-generic `extract_from` could not construct its own payload. Moving to `Parsed` is
/// `resolve`'s job below, which is exactly the separation the typestate exists to draw.
impl<'ast> Extractor<'ast, &'ast Attribute> for SyntaxFieldAttributeExtraction<'ast, Raw> {
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
    pub fn resolve(self) -> Extraction<SyntaxFieldAttributeExtraction<'ast, Parsed>> {
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

impl<'ast, S: Stage> Validate<'ast, &'ast Attribute> for SyntaxFieldAttributeExtraction<'ast, S> {
    type ValidityError = SyntaxFieldAttributeError;

    /// A genuine narrowing, not the input handed back: the head is now RESOLVED to the vocabulary
    /// entry it names, so no later stage repeats the comparison. This is what `Valid` is for.
    type Valid = (&'ast Attribute, SyntaxHelper);

    // Surface-level and nothing more, which is exactly ID(pipeline/validity-scope)'s remit: is
    // this attribute one of ours? No token is interpreted to answer it.
    fn validate(input: &'ast Attribute) -> Result<Self::Valid, Self::ValidityError> {
        SyntaxHelper::try_from(input.path())
            .map(|helper| (input, helper))
            .map_err(|_| SyntaxFieldAttributeError)
    }
}

pub struct SyntaxFieldAttributeError;
