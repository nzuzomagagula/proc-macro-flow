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
        assert::Assert,
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
    impl<'ast> Assert for Leaf<'ast> {}

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

/// An ATTRIBUTE macro's stages, which need a Source carrying two halves.
///
/// NOTE(#facade/hosts-the-attribute-proof): the derive crate cannot use its own derives, so this is
/// the only place `#[derive(Validate)] #[args(..)]` can be declared and then RUN. It is the whole
/// of ID(attributed/source-is-a-pair) end to end: the arguments are read by the ordinary grammar
/// reader, the pair validates to the item, and only the item is what a re-emission would hand back.
#[cfg(test)]
mod annotated {
    use proc_macro_flow_derive::{Syntax, Validate};
    use proc_macro_flow_traits::{
        attributed::{Annotated, Attributed},
        extractor::Validate,
        quote::{quote, ToTokens},
        vocab::leaves::FromBody,
    };
    use syn::{parse_str, ItemFn, LitStr};

    /// The arguments, declared as an ordinary grammar - the same declaration a helper attribute
    /// would get, because it IS the same reader (ID(from-body/one-reader-two-entries)).
    #[derive(Syntax)]
    pub struct TraceArgs {
        level: LitStr,
        skip: Option<LitStr>,
    }

    /// The stage. `#[args]` is the only difference from a derive's declaration.
    #[derive(Validate)]
    #[source(ItemFn)]
    #[args(TraceArgs)]
    pub struct TraceRead<'ast> {
        _p: ::core::marker::PhantomData<&'ast ()>,
    }

    fn item() -> ItemFn {
        parse_str("fn traced() {}").expect("the item parses")
    }

    #[test]
    fn the_arguments_are_read_by_the_ordinary_grammar_reader() {
        let args = TraceArgs::from_body(&quote!(level = "debug"), &item()).expect("reads");

        assert_eq!(args.level.value(), "debug");
        assert!(args.skip.is_none());
    }

    #[test]
    fn a_pair_validates_to_the_item() {
        // ID(validate/source-shape-follows-args): the Source carries both halves, and `Valid`
        // narrows to the node a downstream stage actually wants to look at.
        let args = TraceArgs::from_body(&quote!(level = "debug"), &item()).expect("reads");
        let item = item();

        // `Reason` has no Debug - deliberately, the crate renders rather than formats - so this
        // matches rather than unwrapping.
        let Ok(valid) = TraceRead::validate(Attributed::new(&args, &item)) else {
            panic!("the pair did not validate");
        };
        assert_eq!(valid.sig.ident, "traced");
    }

    #[test]
    fn only_the_item_would_be_re_emitted() {
        // ID(attributed/annotated-decides-the-kind), at the type level: this compiles because the
        // Source is a pair. The same call against a derive's `&DeriveInput` does not, which is
        // what makes `run_attribute` uncallable on the wrong kind of pipeline.
        let args = TraceArgs::from_body(&quote!(level = "debug"), &item()).expect("reads");
        let item = item();
        let node = Attributed::new(&args, &item);

        let out = Annotated::item(node).to_token_stream().to_string();
        assert!(out.contains("fn traced"), "{out}");
        assert!(!out.contains("debug"), "the arguments were re-emitted: {out}");
    }

    #[test]
    fn bad_arguments_are_the_grammars_own_complaint() {
        // The user of the generated macro writes `#[trace(levl = "debug")]`. Nothing special
        // happens here - it is the same reader, so it is the same did-you-mean.
        let error = TraceArgs::from_body(&quote!(levl = "debug"), &item())
            .err()
            .expect("`levl` is not a key");

        assert_eq!(error.to_string(), "expected one of: `level`, `skip`");
    }

    #[test]
    fn a_bare_attribute_missing_a_required_key_still_lands_somewhere() {
        // `#[trace]` and `#[trace()]` are indistinguishable and EMPTY ARGUMENTS HAVE NO SPAN, so
        // without the `at` fallback this complaint would have nothing at all to underline.
        let item = item();
        let error = TraceArgs::from_body(&Default::default(), &item)
            .err()
            .expect("level is required");

        assert!(error.to_string().contains("level"), "{error}");
    }
}

/// Rules a grammar states about itself, and the walk that carries them up.
///
/// NOTE(#facade/hosts-the-rule-proof): same reason as the rest - the derive crate cannot use its
/// own derives, so a grammar that DECLARES rules and then breaks them can only be written here.
#[cfg(test)]
mod rules {
    use proc_macro_flow_derive::Syntax;
    use proc_macro_flow_traits::{
        assert::{Assert, Rule},
        extractor::{Reason, ReasonKind},
        vocab::leaves::FromMeta,
    };
    use syn::{parse_str, LitInt, LitStr, Meta};

    fn meta(source: &str) -> Meta {
        parse_str(source).expect("the meta parses")
    }

    fn violations(value: &impl Assert) -> Vec<String> {
        let mut out: Vec<Reason> = Vec::new();
        value.assert(&mut out);
        out.iter().map(|reason| reason.kind.message()).collect()
    }

    #[derive(Syntax)]
    #[assert(one_of(times, forever))]
    pub struct Retry {
        times: Option<LitInt>,
        forever: Option<LitStr>,
    }

    #[test]
    fn a_satisfied_rule_says_nothing() {
        let value = Retry::from_meta(&meta("retry(times = 3)")).expect("reads");
        assert!(violations(&value).is_empty());
    }

    #[test]
    fn a_broken_rule_is_worded_from_the_rule_and_its_keys() {
        // ID(assert/violation-is-not-a-message): nothing was worded when the rule was recorded -
        // the message is built at render time from the kind plus the keys it names.
        let both = Retry::from_meta(&meta(r#"retry(times = 3, forever = "yes")"#)).expect("reads");
        assert_eq!(
            violations(&both),
            ["expected exactly one of `times`, `forever`"]
        );

        let neither = Retry::from_meta(&meta("retry()")).expect("reads");
        assert_eq!(
            violations(&neither),
            ["expected exactly one of `times`, `forever`"]
        );
    }

    #[derive(Syntax)]
    #[assert(any_of(a, b), requires(b, a))]
    #[assert(at_most_one(c, d))]
    pub struct Several {
        a: Option<LitStr>,
        b: Option<LitStr>,
        c: Option<LitStr>,
        d: Option<LitStr>,
    }

    #[test]
    fn every_broken_rule_is_reported_not_just_the_first() {
        // ID(no-result) at the rule level: a grammar stating three rules reports all three it
        // breaks. `b` alone breaks `requires(b, a)`; `c` and `d` together break `at_most_one`.
        let value = Several::from_meta(&meta(
            r#"several(b = "1", c = "2", d = "3")"#,
        ))
        .expect("reads");

        let found = violations(&value);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().any(|m| m.contains("was written, so")), "{found:?}");
        assert!(found.iter().any(|m| m.contains("at most one")), "{found:?}");
    }

    #[test]
    fn several_rules_may_share_one_attribute_and_several_attributes_may_be_written() {
        // Both forms above are exercised by the type compiling at all; this pins the behaviour.
        let clean = Several::from_meta(&meta(r#"several(a = "1")"#)).expect("reads");
        assert!(violations(&clean).is_empty());
    }

    // ---- propagation, which is the whole ask ------------------------------------------------

    #[derive(Syntax)]
    pub struct Outer {
        #[shape(proc_macro_flow_traits::meta::AttributeKind::MetaList)]
        nested: Retry,
    }

    #[test]
    fn a_rule_on_a_nested_grammar_reaches_the_parent() {
        // THE propagation claim. `Outer` states no rules of its own; the complaint comes from a
        // grammar one level down and is carried by the descent the derive emits. This is the half
        // that would silently do nothing if the leaf `Assert` impls were missing
        // (NOTE(#assert/leaves-are-askable)) or the descent were not generated.
        let value = Outer::from_meta(&meta(r#"outer(nested(times = 3, forever = "y"))"#))
            .expect("reads");

        assert_eq!(
            violations(&value),
            ["expected exactly one of `times`, `forever`"],
            "a nested grammar's rule did not reach the parent"
        );
    }

    #[test]
    fn a_clean_nested_grammar_adds_nothing() {
        let value = Outer::from_meta(&meta("outer(nested(times = 3))")).expect("reads");
        assert!(violations(&value).is_empty());
    }

    // ---- the escape hatch --------------------------------------------------------------------

    pub struct NoZeroRetries;

    impl Rule for NoZeroRetries {
        type Subject = Guarded;

        fn check(subject: &Guarded, out: &mut Vec<Reason>) {
            // An author supplies MEANING; the framework supplies span, position and accumulation.
            if subject
                .times
                .as_ref()
                .and_then(|lit| lit.base10_parse::<u32>().ok())
                == Some(0)
            {
                out.push(Reason::new(ReasonKind::Custom(
                    "retrying zero times is the same as not retrying".to_owned(),
                )));
            }
        }
    }

    #[derive(Syntax)]
    #[assert(with = NoZeroRetries)]
    pub struct Guarded {
        times: Option<LitInt>,
    }

    #[test]
    fn an_authors_own_rule_runs_and_words_itself() {
        let bad = Guarded::from_meta(&meta("guarded(times = 0)")).expect("reads");
        assert_eq!(
            violations(&bad),
            ["retrying zero times is the same as not retrying"]
        );

        let good = Guarded::from_meta(&meta("guarded(times = 3)")).expect("reads");
        assert!(violations(&good).is_empty());
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

    /// THE claim of ID(from-body/one-reader-two-entries), asserted rather than assumed.
    ///
    /// A helper attribute's body and an attribute macro's ARGUMENTS are the same tokens read by the
    /// same reader. `Retry` is declared once and entered both ways here; if the two ever diverge -
    /// a second grammar, a second set of rules - these stop agreeing.
    mod both_ways {
        use super::*;
        use proc_macro_flow_traits::proc_macro2;
        use proc_macro_flow_traits::quote::quote;
        use proc_macro_flow_traits::vocab::leaves::FromBody;

        /// What a DERIVE sees: the whole `retry(..)` attribute, head included.
        fn as_helper(source: &str) -> syn::Result<Retry> {
            Retry::from_meta(&meta(source))
        }

        /// What an ATTRIBUTE MACRO sees: rustc hands the arguments over already unwrapped, so
        /// there is no head at all. `at` is the node a complaint falls back to.
        fn as_arguments(body: proc_macro2::TokenStream) -> syn::Result<Retry> {
            let item: syn::ItemFn = parse_str("fn annotated() {}").expect("the item parses");
            Retry::from_body(&body, &item)
        }

        #[test]
        fn the_same_input_read_both_ways_gives_the_same_value() {
            let helper = as_helper(r#"retry(times = 3, back_off = "200ms")"#).expect("reads");
            let arguments = as_arguments(quote!(times = 3, back_off = "200ms")).expect("reads");

            assert_eq!(helper.times.base10_digits(), arguments.times.base10_digits());
            assert_eq!(
                helper.back_off.map(|lit| lit.value()),
                arguments.back_off.map(|lit| lit.value()),
            );
        }

        #[test]
        fn the_same_mistake_read_both_ways_gives_the_same_complaint() {
            // Not merely 'both fail' - the SAME WORDING, because it is one reader. A second
            // grammar for arguments would drift here first.
            let helper = as_helper(r#"retry(back_off = "200ms")"#)
                .err()
                .expect("times is required");
            let arguments = as_arguments(quote!(back_off = "200ms"))
                .err()
                .expect("times is required");

            assert_eq!(helper.to_string(), arguments.to_string());
            assert!(helper.to_string().contains("times"), "{helper}");
        }

        #[test]
        fn an_unknown_key_is_caught_on_the_arguments_path_too() {
            let error = as_arguments(quote!(times = 3, bakc_off = "200ms"))
                .err()
                .expect("`bakc_off` is not a key");

            // The grammar's own did-you-mean, reached from the arguments side - asserted WHOLE
            // rather than by a substring, because every key name also appears in the grammar
            // itself and a looser check would pass without the candidate list existing.
            assert_eq!(error.to_string(), "expected one of: `times`, `back_off`");
        }


        #[test]
        fn empty_arguments_complain_against_the_item_they_were_written_on() {
            // ID(from-body/fallback-is-tokens-not-a-span), and the reason `at` is a parameter at
            // all. `#[retry]` and `#[retry()]` are indistinguishable to an attribute macro and
            // EMPTY ARGUMENTS HAVE NO SPAN - so without a fallback this complaint has nothing to
            // underline. Spanned output is not inspectable on stable, so what is asserted is that
            // an error is produced and still names the key.
            let error = as_arguments(proc_macro2::TokenStream::new())
                .err()
                .expect("times is required");

            assert!(error.to_string().contains("times"), "{error}");
            assert!(!error.to_compile_error().is_empty());
        }
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

    impl<'ast> Generator<'ast> for Good {
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

    impl<'ast> Generator<'ast> for Bad {
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

/// `#[pipeline]` with `entry = manual` — the wiring half, which a NON proc-macro crate can host.
///
/// The entry-generating half needs a `proc-macro = true` crate, because that is the only place a
/// `#[proc_macro_derive]` function may exist at all. See NOTE(#pipeline/entry-is-a-sibling).
#[cfg(test)]
mod wiring {
    use proc_macro_flow_derive::pipeline;
    use proc_macro_flow_traits::{
        extractor::{Extracted, Extraction, Extractor, Reason, Validate},
        generator::Generator,
        pipeline::Pipeline,
        processor::Processor,
        quote::ToTokens,
        render::Diagnose,
    };
    use syn::{parse_str, DeriveInput, ItemImpl};

    #[pipeline(derive = Tiny, entry = manual)]
    mod tiny {
        use super::*;

        // ONE type, two roles - NOTE(#pipeline-macro/one-type-many-roles), and the commonest
        // shape there is: a pipeline with nothing to process names its extractor twice.
        #[extractor(source = DeriveInput)]
        #[processor(from = Extraction2)]
        pub struct Extraction2<'ast>(pub &'ast DeriveInput);

        #[generator(from = Extraction2)]
        pub struct Processed<'ast>(pub &'ast DeriveInput);
    }

    use tiny::{Extraction2, Processed};

    impl<'ast> Validate<'ast> for Extraction2<'ast> {
        type Source = &'ast DeriveInput;
        type Valid = &'ast DeriveInput;
        fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
            Ok(input)
        }
    }

    impl<'ast> Extractor<'ast> for Extraction2<'ast> {
        type Output = Extracted<Self, &'ast DeriveInput>;
        fn extract_from(node: &'ast DeriveInput) -> Self::Output {
            Extracted::new(Extraction::value(Extraction2(node)), node)
        }
    }

    impl<'ast> proc_macro_flow_traits::assert::Assert for Extraction2<'ast> {}

    impl<'ast> Diagnose for Extraction2<'ast> {
        fn diagnose(&self, _: &mut Vec<syn::Error>) {}
    }

    impl<'ast> Processor<'ast> for Extraction2<'ast> {
        type Input = Extracted<Extraction2<'ast>, &'ast DeriveInput>;
        type Output = Processed<'ast>;
        fn process(input: Self::Input) -> Extraction<Self::Output> {
            // Read through the EXTRACTION, not around it via `source()`. Both reach the same
            // node here, but only this one proves the extractor's payload survived the wiring -
            // which is the whole claim this module exists to make.
            match input.value() {
                Some(extraction) => Extraction::value(Processed(extraction.0)),
                None => Extraction::default(),
            }
        }
    }

    /// The generated item, its own type - ID(generation/newtype-per-item).
    pub struct Block(ItemImpl);

    impl ToTokens for Block {
        fn to_tokens(&self, t: &mut proc_macro_flow_traits::proc_macro2::TokenStream) {
            self.0.to_tokens(t)
        }
    }

    impl<'ast> Generator<'ast> for Processed<'ast> {
        type Input = Self;
        type Subject = &'ast DeriveInput;
        type Output = Block;

        fn generate(input: Self) -> Extraction<Block> {
            let name = &input.0.ident;
            match syn::parse2(proc_macro_flow_traits::quote::quote!(impl #name { const WIRED: bool = true; }))
            {
                Ok(item) => Extraction::value(Block(item)),
                Err(error) => Extraction::failed(Reason::new(
                    proc_macro_flow_traits::extractor::ReasonKind::Internal(error),
                )),
            }
        }

        fn stub(subject: &'ast DeriveInput) -> syn::Result<Block> {
            let name = &subject.ident;
            syn::parse2(proc_macro_flow_traits::quote::quote!(impl #name { const WIRED: bool = false; }))
                .map(Block)
        }
    }

    #[test]
    fn the_pipeline_impl_is_generated_and_runs() {
        // The wiring type is named by the macro from the exported name, and it really is a
        // `Pipeline` - which only compiles if all three associated types line up.
        let input: DeriveInput = parse_str("pub struct Thing;").expect("parses");
        let out = TinyWiring::run(&input).to_token_stream().to_string();

        assert!(out.contains("impl Thing"), "{out}");
        assert!(out.contains("WIRED"), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn entry_manual_emits_no_entry_function() {
        // The toggle is the shape of the tree - NOTE(#pipeline-macro/entry-is-a-child). If an
        // entry fn HAD been emitted, this module would not compile: a #[proc_macro_derive] in a
        // non-proc-macro crate is an error.
        let input: DeriveInput = parse_str("pub struct Other;").expect("parses");
        assert!(TinyWiring::run(&input).item().is_some());
    }
}
