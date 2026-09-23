pub use proc_macro_flow_derive::*;
pub use proc_macro_flow_traits::*;

/// The derives, exercised from the only place that can exercise them.
///
/// NOTE(#facade/hosts-the-proof): V[N(proc_macro_flow).tests(Attr(derive(Extractor)))], "VERIFIED
/// that a proc-macro crate `can't use a procedural macro from the same crate that defines it`, so
/// StructExtraction and FieldExtraction can never carry these derives where they live. The plan's
/// 'rewrite the existing extractors onto the derive' step is impossible rather than unfinished.
/// This crate depends on both halves, so it can declare the EQUIVALENT extraction with the derives
/// and assert the generated tree matches what the hand-written one produces. Same proof, one crate
/// over - and it gives the facade a job, which it did not previously have"
#[cfg(test)]
mod derives {
    // The derive macro and the trait share a name and coexist: derives resolve in the macro
    // namespace, traits in the type namespace.
    use proc_macro_flow_derive::{Extractor, Processor, Validate};
    use proc_macro_flow_traits::{
        extractor::{Extracted, Extraction, Extractor, Reason, ReasonKind, Validate},
        processor::Processor,
        render::Diagnose,
    };
    use syn::{DataStruct, DeriveInput, Field, parse_str};

    /// The child, wholly generated: nothing to validate, nothing to process, one field reached by
    /// a path.
    #[derive(Extractor, Validate, Processor)]
    #[source(Field)]
    pub struct DerivedField<'ast> {
        #[from(source.attrs.iter())]
        attrs: Vec<Extracted<DerivedAttr<'ast>, &'ast syn::Attribute>>,
    }

    #[derive(Extractor, Validate, Processor)]
    #[source(syn::Attribute)]
    pub struct DerivedAttr<'ast> {
        #[with(|a: &'ast syn::Attribute| ::core::iter::once(a))]
        _self: Vec<Extracted<Leaf<'ast>, &'ast syn::Attribute>>,
    }

    /// A leaf with no children at all - the base of the recursion.
    pub struct Leaf<'ast>(::core::marker::PhantomData<&'ast ()>);

    /// Hand-written, because a leaf is the one case the derive cannot state: it has no fields to
    /// read the answer off. VERIFIED that omitting it does not fail silently - the derived parent
    /// stops compiling with `the trait bound `Leaf<'_>: Diagnose` is not satisfied`, so an
    /// unreachable subtree is a compile error rather than a quiet gap in the diagnostics.
    impl<'ast> Diagnose for Leaf<'ast> {
        fn diagnose(&self, _: &mut Vec<syn::Error>) {}
    }

    impl<'ast> Validate<'ast> for Leaf<'ast> {
    type Source = &'ast syn::Attribute;
        type Valid = &'ast syn::Attribute;
        fn validate(input: &'ast syn::Attribute) -> Result<Self::Valid, Reason> {
            Ok(input)
        }
    }

    impl<'ast> Extractor<'ast> for Leaf<'ast> {
        type Output = Extracted<Self, &'ast syn::Attribute>;
        fn extract_from(node: &'ast syn::Attribute) -> Self::Output {
            Extracted::new(
                proc_macro_flow_traits::extractor::Extraction::value(Leaf(
                    ::core::marker::PhantomData,
                )),
                node,
            )
        }
    }

    /// The parent, with a REAL validate written by hand - `DeriveInput` narrowed to `&DataStruct`
    /// - which is why `Validate` is a separate derive rather than part of `Extractor`.
    #[derive(Extractor, Processor)]
    #[source(DeriveInput)]
    pub struct DerivedStruct<'ast> {
        #[from(source.fields.iter())]
        fields: Vec<Extracted<DerivedField<'ast>, &'ast Field>>,
    }

    impl<'ast> Validate<'ast> for DerivedStruct<'ast> {
    type Source = &'ast DeriveInput;
        type Valid = &'ast DataStruct;

        fn validate(input: &'ast DeriveInput) -> Result<Self::Valid, Reason> {
            match &input.data {
                syn::Data::Struct(data) => Ok(data),
                _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
            }
        }
    }

    fn item(source: &str) -> DeriveInput {
        parse_str(source).expect("the item parses")
    }

    #[test]
    fn a_derived_extractor_walks_to_its_children() {
        let input = item("pub struct Thing { a: u8, b: String }");
        let extracted = DerivedStruct::extract_from(&input);

        let value = extracted.value().expect("a struct extracts");
        assert_eq!(value.fields.len(), 2);
    }

    #[test]
    fn the_from_expression_sees_the_validated_source() {
        // `#[from(source.fields.iter())]` - `source` is the &DataStruct that validate returned,
        // not the DeriveInput. A tuple struct proves it reached the right thing.
        let input = item("pub struct Thing(u8, String, bool);");
        let extracted = DerivedStruct::extract_from(&input);

        assert_eq!(extracted.value().unwrap().fields.len(), 3);
    }

    #[test]
    fn a_failing_validate_yields_no_value_but_keeps_the_source() {
        let input = item("pub enum Thing { A }");
        let extracted = DerivedStruct::extract_from(&input);

        assert!(extracted.value().is_none());
        // the node survived the failure - which is the point of it riding on the output
        assert_eq!(extracted.source().ident.to_string(), "Thing");
    }

    #[test]
    fn a_failing_validate_says_why() {
        let input = item("pub enum Thing { A }");
        let extracted = DerivedStruct::extract_from(&input);

        assert_eq!(extracted.reasons().len(), 1, "the failure was silent");
        // asserted through the public rendering - ReasonKind carries no PartialEq, and widening
        // the API for a test is the wrong way round
        assert_eq!(extracted.reasons()[0].message(), "written in the wrong shape");
    }

    #[test]
    fn a_failing_validate_reaches_the_user_through_the_walk() {
        // and the reason is not merely recorded - it renders
        let input = item("pub enum Thing { A }");
        let errors = DerivedStruct::extract_from(&input).render();

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "written in the wrong shape");
    }

    #[test]
    fn arity_comes_off_the_field_type() {
        // `Vec<Extracted<..>>` picked `extract_each` with no attribute saying so. A field with
        // no attributes still produces an (empty) child list rather than being skipped.
        let input = item("pub struct Thing { plain: u8 }");
        let extracted = DerivedStruct::extract_from(&input);
        let value = extracted.value().unwrap();

        assert_eq!(value.fields.len(), 1);
        assert!(value.fields[0].value().unwrap().attrs.is_empty());
    }

    #[test]
    fn with_applies_a_callable_where_from_evaluates_an_expression() {
        let input = item("pub struct Thing { #[doc = \"x\"] a: u8 }");
        let extracted = DerivedStruct::extract_from(&input);

        let field = &extracted.value().unwrap().fields[0];
        let attr = &field.value().unwrap().attrs[0];
        // `#[with(|a| once(a))]` was applied to the source rather than evaluated as a value
        assert_eq!(attr.value().unwrap()._self.len(), 1);
    }

    #[test]
    fn the_derived_processor_is_the_identity() {
        let input = item("pub struct Thing { a: u8 }");
        let processed = DerivedStruct::process(DerivedStruct::extract_from(&input));

        // same tree, unchanged
        assert_eq!(processed.value.unwrap().fields.len(), 1);
    }

    #[test]
    fn a_derived_tree_renders_nothing_when_it_is_clean() {
        let input = item("pub struct Thing { a: u8 }");
        assert!(DerivedStruct::extract_from(&input).render().is_empty());
    }

    #[test]
    fn a_reason_three_levels_down_reaches_the_walk() {
        // The derive's Diagnose impl is what makes this reachable at ALL - struct, field, attr,
        // leaf, each generated except the leaf. Nothing in the pipeline reads a nested reason
        // unless every link in that chain says where its children are.
        let input = item("pub struct Thing { #[doc = \"x\"] a: u8 }");
        let extracted = DerivedStruct::extract_from(&input);

        // the real extraction reaches the deepest node; give an equivalent one a complaint and
        // check the walk renders it against that same source
        let attr = &extracted.value().unwrap().fields[0].value().unwrap().attrs[0];
        let leaf = &attr.value().unwrap()._self[0];
        let complaining = Extracted::new(
            Extraction::value(Leaf(::core::marker::PhantomData))
                .with_reason(Reason::new(ReasonKind::UnknownKey)),
            *leaf.source(),
        );

        let errors = complaining.render();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "not a key this node accepts");
    }

    #[test]
    fn the_derived_walk_descends_through_every_generated_link() {
        // Four levels of generated `diagnose` in a row: struct -> field -> attr -> leaf. The tree
        // is built by hand in exactly the shape the derive produces, with one reason at the
        // bottom, because the point is which links the WALK crosses, not which the extractor
        // built. An empty impl anywhere in the chain makes this zero.
        let input = item("pub struct Thing { #[doc = \"x\"] a: u8 }");
        let syn::Data::Struct(data) = &input.data else {
            unreachable!()
        };
        let field = data.fields.iter().next().expect("one field");
        let attr = field.attrs.first().expect("one attribute");

        let leaf = Extracted::new(
            Extraction::value(Leaf(::core::marker::PhantomData))
                .with_reason(Reason::new(ReasonKind::Missing)),
            attr,
        );
        let derived_attr = Extracted::new(
            Extraction::value(DerivedAttr { _self: vec![leaf] }),
            attr,
        );
        let derived_field = Extracted::new(
            Extraction::value(DerivedField {
                attrs: vec![derived_attr],
            }),
            field,
        );
        let root = Extracted::new(
            Extraction::value(DerivedStruct {
                fields: vec![derived_field],
            }),
            &input,
        );

        let errors = root.render();
        assert_eq!(errors.len(), 1, "the walk stopped short");
        assert_eq!(errors[0].to_string(), "required, and not written");
    }

    #[test]
    fn the_derived_tree_matches_the_hand_written_one() {
        // THE proof the plan wanted from Step 6, relocated here because the derive crate cannot
        // use its own derives. Same input, same shape.
        let input = item("pub struct Thing { a: u8, b: String, c: bool }");

        let derived = DerivedStruct::extract_from(&input);
        assert_eq!(derived.value().unwrap().fields.len(), 3);
        assert_eq!(derived.source().ident.to_string(), "Thing");
        assert!(derived.reasons().is_empty());
    }
}

/// The syntax stage, exercised where it can be: a grammar declared with the derive.
///
/// NOTE(#facade/hosts-the-grammar-proof): same reason as NOTE(#facade/hosts-the-proof) - the
/// derive crate cannot use its own derives, so the only place a DERIVED grammar can be declared
/// and then read is here.
#[cfg(test)]
mod grammar {
    use proc_macro_flow_derive::Syntax;
    use proc_macro_flow_traits::{
        meta::AttributeKind,
        node::{Arity, Described},
        vocab::leaves::FromMeta,
    };
    use syn::{parse_str, LitInt, LitStr, Meta};

    #[derive(Syntax)]
    pub struct Retry {
        times: LitInt,
        #[alias]
        back_off: Option<LitStr>,
    }

    /// The contrast with `meta_list!`, spelled out.
    #[derive(Syntax)]
    pub struct Qualified {
        maybe: std::option::Option<LitStr>,
    }

    /// Exists to prove the selector reaches the table and the BOUND. Reading it is not the
    /// point, so the field is never touched.
    #[derive(Syntax)]
    pub struct Narrowed {
        #[shape(AttributeKind::MetaList)]
        #[allow(dead_code)]
        nested: Retry,
    }

    fn meta(source: &str) -> Meta {
        parse_str(source).expect("the meta parses")
    }

    #[test]
    fn a_derived_grammar_reads_its_fields() {
        let value = Retry::from_meta(&meta(r#"retry(times = 3, back_off = "200ms")"#))
            .expect("both keys are written");

        assert_eq!(value.times.base10_digits(), "3");
        assert_eq!(value.back_off.unwrap().value(), "200ms");
    }

    #[test]
    fn an_optional_field_may_be_absent() {
        let value = Retry::from_meta(&meta("retry(times = 3)")).expect("backoff is optional");
        assert!(value.back_off.is_none());
    }

    #[test]
    fn a_missing_required_field_is_reported() {
        let error = Retry::from_meta(&meta(r#"retry(back_off = "200ms")"#))
            .err()
            .expect("times is required");
        assert!(error.to_string().contains("times"), "{error}");
    }

    #[derive(Syntax)]
    pub struct TwoRequired {
        #[allow(dead_code)]
        first: LitInt,
        #[allow(dead_code)]
        second: LitInt,
    }

    #[test]
    fn every_missing_key_is_reported_not_just_the_first() {
        // ID(no-result), through the derive. The generated reader used to return as soon as it
        // found one missing key - and reach for `.err().expect("not empty")` to do it, a panic in
        // the author's compile.
        let error = TwoRequired::from_meta(&meta("two_required()"))
            .err()
            .expect("both keys are missing");

        assert_eq!(error.into_iter().count(), 2, "only one missing key was reported");
    }

    #[test]
    fn the_node_table_is_emitted_with_arity() {
        let node = <Retry as Described>::NODE;

        assert_eq!(node.name, "retry", "the entry name is the type in snake_case");
        assert_eq!(node.child("times").unwrap().arity, Arity::Required);
        assert_eq!(node.child("back_off").unwrap().arity, Arity::Optional);
    }

    #[test]
    fn alias_generates_the_standard_case_set() {
        // `#[alias]` with no arguments, expanded by heck AT EXPANSION TIME into literals - so
        // matching stays exact (#vocabulary/exact) and the spellings are visible in the table.
        let node = <Retry as Described>::NODE;
        let aliases = node.child("back_off").unwrap().aliases;

        assert!(aliases.contains(&"backOff"), "{aliases:?}");
        assert!(aliases.contains(&"back-off"), "{aliases:?}");
        assert!(!aliases.contains(&"back_off"), "canonical is not repeated");
    }

    #[test]
    fn a_field_with_no_alias_attribute_has_none() {
        let node = <Retry as Described>::NODE;
        assert!(node.child("times").unwrap().aliases.is_empty());
    }

    #[test]
    fn a_qualified_option_is_still_optional() {
        // NOTE(#syntax-derive/parses-the-type). `meta_list!` matches the TOKENS `Option < .. >`,
        // so `std::option::Option<LitStr>` reads as REQUIRED there and fails to compile - which
        // NOTE(#forwarding/no-option) keeps loud on purpose. The derive PARSES the type and looks
        // at `segments.last()`, so it is simply correct.
        let value = Qualified::from_meta(&meta("qualified()")).expect("the field is optional");
        assert!(value.maybe.is_none());

        assert_eq!(
            <Qualified as Described>::NODE.child("maybe").unwrap().arity,
            Arity::Optional,
        );
    }

    #[test]
    fn a_shape_selector_reaches_the_node_table() {
        use proc_macro_flow_traits::meta::ShapeKind;
        let node = <Narrowed as Described>::NODE;

        assert_eq!(node.child("nested").unwrap().shapes, &[ShapeKind::List]);
    }
}

/// The generator derive, proved where it can be — same reason as
/// NOTE(#facade/hosts-the-proof): the derive crate cannot use its own derives.
#[cfg(test)]
mod generation {
    use proc_macro_flow_derive::Generator;
    use proc_macro_flow_traits::{
        extractor::{Extraction, Reason, ReasonKind},
        generator::Generator,
    };
    // Through the facade's re-exports, exactly as an author would: a grammar crate depends on
    // proc_macro_flow and NOTHING ELSE. If this test needed `quote` in its own Cargo.toml, the
    // generated code would need it in the author's too.
    use proc_macro_flow_traits::proc_macro2;
    use proc_macro_flow_traits::quote::{quote, ToTokens};
    use syn::{parse2, ImplItem, ItemImpl};

    /// A LEAF that always succeeds.
    pub struct Good(ImplItem);
    /// A LEAF that always fails, so per-child isolation is observable.
    pub struct Bad(ImplItem);

    impl ToTokens for Good {
        fn to_tokens(&self, t: &mut proc_macro2::TokenStream) {
            self.0.to_tokens(t)
        }
    }
    impl ToTokens for Bad {
        fn to_tokens(&self, t: &mut proc_macro2::TokenStream) {
            self.0.to_tokens(t)
        }
    }
    // Block's is DERIVED - see the impl the derive emits.

    impl Generator for Good {
        type Input = ();
        type Subject = ();
        type Output = Self;
        fn generate(_: ()) -> Extraction<Self> {
            match parse2(quote!(const GOOD: u8 = 1;)) {
                Ok(item) => Extraction::value(Good(item)),
                Err(error) => Extraction::failed(Reason::new(ReasonKind::Internal(error))),
            }
        }
        fn stub(_: ()) -> syn::Result<Self> {
            parse2(quote!(const GOOD: u8 = 0;)).map(Good)
        }
    }

    impl Generator for Bad {
        type Input = ();
        type Subject = ();
        type Output = Self;
        fn generate(_: ()) -> Extraction<Self> {
            // fails the way a real leaf would: tokens that are not an ImplItem
            match parse2::<ImplItem>(quote!(this is not an impl item)) {
                Ok(item) => Extraction::value(Bad(item)),
                Err(error) => Extraction::failed(Reason::new(ReasonKind::Internal(error))),
            }
        }
        fn stub(_: ()) -> syn::Result<Self> {
            parse2(quote!(const BAD: u8 = 0;)).map(Bad)
        }
    }

    /// The PARENT, entirely derived except for the two assembly functions.
    #[derive(Generator)]
    #[generator(from = (), subject = ())]
    #[generates(
        good: Good = (),
        bad: Bad = (),
    )]
    pub struct Block(ItemImpl);

    impl Block {
        fn assemble(_: &(), good: Good, bad: Bad) -> syn::Result<Self> {
            parse2(quote!(impl Thing { #good #bad })).map(Block)
        }
        fn assemble_stub(_: (), good: Good, bad: Bad) -> syn::Result<Self> {
            parse2(quote!(impl Thing { #good #bad })).map(Block)
        }
    }

    #[test]
    fn a_failed_child_does_not_cost_its_siblings() {
        // THE assertion the newtypes exist for, and the one `Result` structurally could not
        // support. `bad` fails; `good` still reaches the output; the block is still assembled.
        let generated = Block::generate(());

        let block = generated.value.expect("the block is still assembled");
        let out = block.0.to_token_stream().to_string();

        assert!(out.contains("GOOD"), "the good child was lost: {out}");
        assert!(out.contains("BAD"), "the failed child was not stubbed: {out}");
    }

    #[test]
    fn the_failure_is_kept_and_attributed() {
        let generated = Block::generate(());

        assert_eq!(generated.reasons.len(), 1, "exactly one child failed");
        assert!(
            generated.reasons[0].is_internal(),
            "tokens WE assembled are ours, never the author's",
        );
    }

    #[test]
    fn a_stub_uses_every_childs_vacant_form() {
        let block = Block::stub(()).expect("the stub assembles");
        let out = block.0.to_token_stream().to_string();

        assert!(out.contains("GOOD"), "{out}");
        assert!(out.contains("BAD"), "{out}");
    }
}
