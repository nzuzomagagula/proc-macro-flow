// @review [ ]
//! The worked example, end to end, in three layers.
//!
//! `mod.rs` states the design and `scratch` is the maximal grammar; neither shows what the pipeline
//! actually DOES with one. This file is that walkthrough, and it is deliberately split by what is
//! real today:
//!
//! | Layer | Lives in | Status |
//! |---|---|---|
//! | 1. what the grammar AUTHOR declares | [`author`] | gated - needs ID(syntax/traits) |
//! | 2. what the downstream USER writes | [`downstream`] | gated - needs ID(syntax/derive) |
//! | 3. what the PIPELINE does with it | `walkthrough` (tests) | **runs, and is asserted** |
//!
//! Layer 3 is the honest part: it exercises the code that exists, so if this file stops telling the
//! truth the test suite says so. Layers 1 and 2 are `#[cfg(any())]` for the same reason `scratch`
//! is - they name items nobody has written yet.
//!
//! ---------------------------------------------------------------------------
//! THE SEAM, and the thing to understand before reading further
//! ---------------------------------------------------------------------------
//!
//! This file previously drew the extraction and syntax stages as two rival attribute readers that
//! needed joining. That was a stage-boundary error. The real division is:
//!
//! ```text
//!   EXTRACTION                          PROCESSING                      GENERATION
//!   walks to nodes and CARRIES them     makes them MEAN something       emits
//!
//!   StructExtraction  <- DeriveInput
//!     └ FieldExtraction <- Field
//!         └ ...Attribute  ──carries──>  grammar reading                 splice + rustc
//!             Unresolved<'ast, T>       Configuration { .. }            checks the selector
//! ```
//!
//! An extractor answers exactly two questions - *what is the source of this extraction* and *how do
//! we get there*. The second is its `Visitable` bound. The first is answered STRUCTURALLY, by
//! `Extracted` carrying the node, rather than by a trait each extractor had to store a copy for -
//! `Sourced` was deleted once that duplication was visible. It never answers "how should this node
//! be parsed": it may hand on a raw `TokenStream`, and understanding that stream is the processor's
//! job, with `resolution::Unresolved` as the carrier.
//!
//! **Consequence for the grammar work.** Reading `colour(ColourSetting::Red)` into a real
//! `ColourSetting` is PROCESSING, not extraction, so grammar types do not implement `Extractor`.
//! That mechanism is built and proven - a real attribute reads into a real `Configuration`, and
//! every rejection case in SCRATCH.md's table is covered - but it belongs against `Processor`
//! (ID(pipeline/base-processor)), which is still a comment-only stub. It is deliberately not forced
//! into this stage to get it into the repo sooner.
//!
//! `TransformationExtraction` is GONE. It was never a node: it held the author's transformation
//! expression - a field path, a closure, a function pointer - which is `#[from]` on the field that
//! needs it. See DEPRECATED(#attribute/generic-grammar) in base/extractor/extractor/field.rs.

// ---------------------------------------------------------------------------
// LAYER 1 - what the grammar author declares
// ---------------------------------------------------------------------------
//
// Gated: `Syntax`, the shape markers and the forwarding impls do not exist. This is the smallest
// grammar that still exercises every mechanism, where `scratch` is the maximal one.
#[cfg(any())]
pub mod author {
    use proc_macro_flow_traits::syntax::{AttributeKind, FromMeta};
    use syn::{Ident, LitStr};

    /// Entry attribute `configuration`, from the type name.
    #[derive(Syntax)]
    #[alias(config)]
    pub struct Configuration {
        /// Required (not `Option`) and many (`Vec`) - both read off the TYPE, never an attribute.
        /// `#[shape]` narrows which written forms are acceptable here; it does not validate.
        #[shape(AttributeKind::MetaList)]
        colour: Vec<ColourSetting>,

        /// Optional, and no `#[shape]` - so any shape `ConfigName` implements is accepted and the
        /// written `Meta` variant selects.
        #[alias(label)]
        name: Option<ConfigName>,
    }

    // NOTE(#no-type-alias): V[Attr(alias).on(E(ColourSetting)) != Seg(Colour)], "There is
    // deliberately no #[alias(Colour)] here making `Colour::Red` resolve. A grammar type is a REAL
    // item that exists once, and inventing a second path segment for it would be a phantom naming
    // nothing - the author's crate would have to grow a `pub use ColourSetting as Colour;` to make
    // it real, which is the duplication the design is avoiding. The principle: THE FRAMEWORK MAY
    // ALIAS WHAT IT OWNS - attribute heads and field keys, which are names we invent and match
    // ourselves - AND MUST NOT ALIAS WHAT RUSTC OWNS, which is paths. A user who wants the short
    // spelling writes `use ColourSetting as Colour;` in their own crate and gets it from rustc,
    // with rustc's own semantics and no feature of ours competing with it"
    #[derive(Syntax)]
    pub enum ColourSetting {
        Red,
        Other(Ident),
    }

    #[derive(Syntax)]
    pub struct ConfigName(LitStr);
}

// ---------------------------------------------------------------------------
// LAYER 2 - what the downstream user writes
// ---------------------------------------------------------------------------
//
// Gated: needs `SomeDerive` and the `configuration` helper registered by ID(syntax/derive).
#[cfg(any())]
pub mod downstream {
    use super::author::Configuration;

    #[derive(SomeDerive)]
    #[configuration(
        colour(ColourSetting::Red, Other(Blue)), // suffix match on the value
        name = "thing",                          // `label = "thing"` is equivalent
    )]
    pub struct Thing;
}

// ---------------------------------------------------------------------------
// LAYER 3 - what the pipeline actually does, today
// ---------------------------------------------------------------------------

#[cfg(test)]
mod walkthrough {
    use proc_macro_flow_traits::{
        extractor::ReasonKind,
        resolution::{Deferred, Raw},
    };
    use quote::ToTokens;
    use syn::{parse_str, Field, ItemStruct};

    use crate::base::syntax::extractor::{
        SyntaxFieldAttributeExtraction, SyntaxFieldAttributeKind,
    };
    use proc_macro_flow_traits::extractor::Extractor;
    use proc_macro_flow_traits::visitable::Visitable;

    /// The author's grammar type as the compiler sees it - a plain struct whose FIELDS carry the
    /// helper attributes. This is what `FieldExtraction` walks.
    fn grammar_field(source: &str) -> Field {
        let item: ItemStruct = parse_str(source).expect("the grammar declaration parses");
        item.fields.into_iter().next().expect("one field")
    }

    #[test]
    fn a_field_carries_its_helper_attributes_into_the_syntax_stage() {
        let field = grammar_field(
            r#"
            pub struct Configuration {
                #[shape(AttributeKind::MetaList)]
                #[alias(colours)]
                colour: Vec<ColourSetting>,
            }
            "#,
        );

        let extractions: Vec<_> = field
            .attrs
            .iter()
            .map(SyntaxFieldAttributeExtraction::<Raw>::extract_from)
            .collect();

        assert_eq!(extractions.len(), 2);
        assert!(extractions.iter().all(|e| e.value().is_some()));
        assert!(extractions.iter().all(|e| e.reasons().is_empty()));

        // Every node can point at what the user wrote - and the source is on the OUTPUT, not on
        // the value, so this works even when extraction produced nothing. That is what made
        // Tr(Sourced) redundant: it asked every extractor to store and return the same node.
        assert!(extractions[0]
            .source()
            .to_token_stream()
            .to_string()
            .contains("shape"));
    }

    #[test]
    fn a_shape_is_carried_verbatim_and_never_interpreted() {
        let field = grammar_field(
            r#"
            pub struct Configuration {
                #[shape(AttributeKind::MetaList)]
                colour: Vec<ColourSetting>,
            }
            "#,
        );

        let extraction = SyntaxFieldAttributeExtraction::<Raw>::extract_from(&field.attrs[0]);
        let node = extraction.value().unwrap();

        let SyntaxFieldAttributeKind::Shape(shape) = node.kind() else {
            panic!("the `shape` head selects the Shape arm");
        };

        // THE property the whole design rests on: the tokens come back out exactly as written,
        // spans and all. Nothing here parsed `AttributeKind::MetaList`, matched it against a table
        // of known variants, or checked that `AttributeKind` is even a real path - the generator
        // splices this into a type position and RUSTC resolves it. That is what buys did-you-mean
        // with a machine-applicable fix, and "but trait FromMeta<MetaList> is implemented for it",
        // neither of which a matcher in this crate could produce.
        assert_eq!(shape.tokens().to_string(), "AttributeKind :: MetaList");
    }

    #[test]
    fn resolving_moves_the_whole_node_from_raw_to_parsed() {
        let field = grammar_field(
            r#"
            pub struct Configuration {
                #[alias(colours)]
                colour: Vec<ColourSetting>,
            }
            "#,
        );

        let raw = SyntaxFieldAttributeExtraction::<Raw>::extract_from(&field.attrs[0])
            .into_extraction()
            .value
            .expect("extraction succeeds");

        // `raw` is consumed here. There is no way to go on using the unresolved form by accident,
        // and no way to resolve the result again - `Parsed` has no `resolve`.
        let parsed = raw.resolve().value.expect("`colours` is an ident");

        let SyntaxFieldAttributeKind::Alias(alias) = parsed.kind() else {
            panic!("the `alias` head selects the Alias arm");
        };
        assert_eq!(alias.value().to_string(), "colours");

        // Resolution never costs the source: still spliceable, still able to point at the input.
        assert_eq!(alias.tokens().to_string(), "colours");
    }

    #[test]
    fn a_selector_that_is_not_even_syntactically_a_type_fails_at_resolution() {
        let field = grammar_field(
            r#"
            pub struct Configuration {
                #[shape(!!)]
                colour: Vec<ColourSetting>,
            }
            "#,
        );

        // Extraction SUCCEEDS - it only carried the tokens, it did not read them.
        let raw = SyntaxFieldAttributeExtraction::<Raw>::extract_from(&field.attrs[0])
            .into_extraction()
            .value
            .expect("carrying tokens cannot fail");

        // The complaint appears only when someone asks for meaning.
        let resolved = raw.resolve();
        assert!(resolved.value.is_none());
        assert!(matches!(resolved.reasons[0].kind, ReasonKind::WrongShape));

        // This reason carries its own span - `resolve` hands back a bare Extraction with no
        // Extracted around it, so there is no chain behind it and it has to. The fallback node is
        // still required by the signature and simply goes unused here; a caller always has one,
        // because it is the caller who held the Extracted in the first place.
        assert!(resolved.reasons[0].span().is_some());
        assert!(!resolved.reasons[0]
            .to_error(&field.attrs[0], "not a shape")
            .to_compile_error()
            .is_empty());
    }

    #[test]
    fn an_unrecognised_head_is_rustcs_complaint_and_not_ours() {
        // `#[shpae(..)]` never reaches this code in a real compilation: a derive registers its
        // helpers with attributes(shape, alias), and rustc rejects every other head first, with a
        // better message than we could write. So when one DOES reach us it belongs to another
        // macro, and the only correct response is silence. See NOTE(#heads-are-rustcs).
        let field = grammar_field(
            r#"
            pub struct Configuration {
                #[shpae(AttributeKind::MetaList)]
                colour: Vec<ColourSetting>,
            }
            "#,
        );

        let extraction = SyntaxFieldAttributeExtraction::<Raw>::extract_from(&field.attrs[0]);

        assert!(extraction.value().is_none());
        assert!(
            extraction.reasons().is_empty(),
            "an unowned head is not ours to complain about"
        );
    }

    #[test]
    fn every_field_attribute_is_visited_and_no_complaint_is_lost() {
        // Two malformed attributes that ARE ours - both written as bare paths where a list is
        // required. The walk does not stop at the first: both reasons come back, which is the
        // property ID(no-result) exists to protect.
        let field = grammar_field(
            r#"
            pub struct Configuration {
                #[shape]
                #[alias]
                colour: Vec<ColourSetting>,
            }
            "#,
        );

        let reasons: Vec<_> = field
            .attrs
            .iter()
            .map(SyntaxFieldAttributeExtraction::<Raw>::extract_from)
            .flat_map(|extracted| extracted.into_extraction().reasons)
            .collect();

        assert_eq!(reasons.len(), 2);
        assert!(reasons
            .iter()
            .all(|r| matches!(r.kind, ReasonKind::WrongShape)));
    }

    #[test]
    fn a_failed_extraction_still_says_where_it_came_from() {
        // The case `T: Sourced` cannot cover, and the whole reason the source lives on the OUTPUT
        // rather than on the extracted value: there is no value here to ask.
        let field = grammar_field(
            r#"
            pub struct Configuration {
                #[shpae(AttributeKind::MetaList)]
                colour: Vec<ColourSetting>,
            }
            "#,
        );

        let extracted = SyntaxFieldAttributeExtraction::<Raw>::extract_from(&field.attrs[0]);

        assert!(extracted.value().is_none());
        assert!(extracted
            .source()
            .to_token_stream()
            .to_string()
            .contains("shpae"));

        // And it can be rendered, spanned under the whole attribute.
        // Rendered through the chain: Extracted holds the node, so a reason with nothing finer
        // to say still lands somewhere real.
        assert!(!extracted
            .to_error("unknown helper attribute")
            .to_compile_error()
            .is_empty());
    }

    #[test]
    fn accept_dispatches_a_node_to_its_visit_method() {
        // `accept` had been dead code since it was written, which is exactly why nobody noticed it
        // was uncallable on anything passed by value. Exercise it so the signature stays honest.
        #[derive(Default)]
        struct Counter {
            attributes: usize,
        }

        impl<'ast> syn::visit::Visit<'ast> for Counter {
            fn visit_attribute(&mut self, _: &'ast syn::Attribute) {
                self.attributes += 1;
            }
        }

        let field = grammar_field(
            r#"
            pub struct Configuration {
                #[shape(AttributeKind::MetaList)]
                colour: Vec<ColourSetting>,
            }
            "#,
        );

        let mut counter = Counter::default();

        // `accept` takes `self`, but Self IS `&'ast Attribute` - a Copy pointer. Nothing is moved
        // and nothing is cloned, which is why the same node can be visited twice and still be
        // borrowed afterwards. Keeping AST references around for later stages stays free.
        let node = &field.attrs[0];
        Visitable::accept(node, &mut counter);
        Visitable::accept(node, &mut counter);

        assert_eq!(counter.attributes, 2);

        // still usable, still the same node
        assert!(node.path().is_ident("shape"));

        // and it can be parked for a later stage at pointer cost
        let kept: Vec<&syn::Attribute> = field.attrs.iter().collect();
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn attributes_belonging_to_other_macros_are_not_our_complaint() {
        // A doc comment IS an attribute (`#[doc = ".."]`), and so is anyone else's registered
        // helper. rustc already rejects a head nobody registered - VERIFIED, with two
        // machine-applicable fixes naming the derive - so anything reaching us with a head we do
        // not own belongs to someone else and must be passed over in silence.
        let field = grammar_field(
            r#"
            pub struct Configuration {
                /// The colours this configuration accepts.
                #[shape(AttributeKind::MetaList)]
                colour: Vec<ColourSetting>,
            }
            "#,
        );

        let reasons: Vec<_> = field
            .attrs
            .iter()
            .map(SyntaxFieldAttributeExtraction::<Raw>::extract_from)
            .flat_map(|extracted| extracted.into_extraction().reasons)
            .collect();

        assert!(reasons.is_empty(), "a doc comment is not an unknown key");
    }
}
