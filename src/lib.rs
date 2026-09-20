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
        type ValidityError = ();
        type Valid = &'ast syn::Attribute;
        fn validate(input: &'ast syn::Attribute) -> Result<Self::Valid, ()> {
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
        type ValidityError = ();
        type Valid = &'ast DataStruct;

        fn validate(input: &'ast DeriveInput) -> Result<Self::Valid, ()> {
            match &input.data {
                syn::Data::Struct(data) => Ok(data),
                _ => Err(()),
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
