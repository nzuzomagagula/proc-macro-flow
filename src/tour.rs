//! A TOUR: every pattern this crate offers, compiled.
//!
//! NOTE(#tour/the-guide-is-compiled): V[N(tour).covers(vocab, grammar, pipeline)], "Written to be
//! read alongside the guide, and kept because prose about a macro rots silently while this does
//! not. It exercises the surface the way an AUTHOR meets it - through the facade and nothing else -
//! which is exactly how it caught ID(vocab/macros-need-the-reexports): M(flag), M(name_value) and
//! M(variants) emitted bare `::quote::` and `::proc_macro2::` paths, so a crate using them had to
//! depend on both by those names. Every other test here imports what it needs directly and could
//! not have noticed."

#![cfg(test)]
#![allow(dead_code)]

// ---------- 1. the vocabulary suite ----------
mod vocab_patterns {
    use proc_macro_flow_traits::{flag, meta_list, name_value, variants, vocabulary};
    use syn::{LitInt, LitStr};

    vocabulary! {
        /// Matched from written tokens; gives TryFrom<&Ident>/<&Path>, ALL, spelling(), candidates().
        pub enum Helper { Shape = "shape", Alias = "alias" | "renamed" }
    }

    flag! {
        /// `#[no_clean]` — presence is the signal.
        pub struct NoClean = "no_clean" | "NoClean";
    }

    name_value! {
        /// `name = "thing"`
        pub struct ConfigName = LitStr;
    }

    meta_list! {
        /// `retry(times = 3, backoff = "200ms")`
        pub struct Retry {
            times: LitInt = "times",
            backoff: Option<LitStr> = "backoff",
        }
    }

    variants! {
        pub enum ColourSetting {
            Red = "Red",
            Other = "Other" (syn::Ident),
            Rgb = "Rgb" { r: LitInt, g: LitInt, b: LitInt },
        }
    }

    #[test]
    fn they_read() {
        use proc_macro_flow_traits::vocab::leaves::FromMeta;
        let m: syn::Meta = syn::parse_str(r#"retry(times = 3, backoff = "200ms")"#).unwrap();
        let r = Retry::from_meta(&m).unwrap();
        assert_eq!(r.times.base10_digits(), "3");
        assert_eq!(Helper::from_spelling("renamed"), Some(Helper::Alias));
    }
}

// ---------- 2. a grammar with the derive ----------
mod grammar_patterns {
    use proc_macro_flow_derive::Syntax;
    use proc_macro_flow_traits::{
        assert::{Assert, Rule},
        extractor::{Reason, ReasonKind},
        meta::AttributeKind,
        node::{Arity, Described},
        vocab::leaves::{FromBody, FromMeta},
    };
    use syn::{LitInt, LitStr, Meta};

    pub struct NonZero;
    impl Rule for NonZero {
        type Subject = Retry;
        fn check(s: &Retry, out: &mut Vec<Reason>) {
            if s.times.as_ref().and_then(|l| l.base10_parse::<u32>().ok()) == Some(0) {
                out.push(Reason::new(ReasonKind::Custom("zero retries is no retries".into())));
            }
        }
    }

    #[derive(Syntax)]
    #[assert(one_of(times, forever), requires(back_off, times))]
    #[assert(with = NonZero)]
    pub struct Retry {
        times: Option<LitInt>,
        forever: Option<LitStr>,
        #[alias]
        back_off: Option<LitStr>,
    }

    #[derive(Syntax)]
    pub struct Outer {
        #[shape(AttributeKind::MetaList)]
        nested: Retry,
        #[alias("label", "tag")]
        name: LitStr,
    }

    fn meta(s: &str) -> Meta { syn::parse_str(s).unwrap() }
    fn broken(v: &impl Assert) -> Vec<String> {
        let mut out = Vec::new();
        v.assert(&mut out);
        out.iter().map(|r| r.kind.message()).collect()
    }

    #[test]
    fn the_table_the_rules_and_both_entries() {
        let node = <Retry as Described>::NODE;
        assert_eq!(node.name, "retry");
        assert_eq!(node.children[0].arity, Arity::Optional);
        assert!(node.candidates().contains("times"));

        let v = Retry::from_meta(&meta("retry(times = 3)")).unwrap();
        assert!(broken(&v).is_empty());

        let both = Retry::from_meta(&meta(r#"retry(times = 3, forever = "y")"#)).unwrap();
        assert_eq!(broken(&both), ["expected exactly one of `times`, `forever`"]);

        let zero = Retry::from_meta(&meta("retry(times = 0)")).unwrap();
        assert_eq!(broken(&zero), ["zero retries is no retries"]);

        // the SAME grammar read as an attribute macro's arguments
        let args = Retry::from_body(
            &"times = 3".parse::<proc_macro_flow_traits::proc_macro2::TokenStream>().unwrap(),
            &meta("anything"),
        ).unwrap();
        assert!(args.times.is_some());

        // nested rules propagate
        let o = Outer::from_meta(&meta(r#"outer(nested(times = 3, forever = "y"), name = "n")"#)).unwrap();
        assert_eq!(broken(&o).len(), 1);
    }
}

// ---------- 3. a pipeline, end to end ----------
mod pipeline_patterns {
    use proc_macro_flow_derive::{pipeline, Extractor, Generator, Processor, Validate};
    use proc_macro_flow_traits::{
        assert::Assert,
        attributed::{Annotated, Attributed},
        extractor::{Extracted, Extraction, Extractor, Reason, ReasonKind, Validate},
        generator::Generator,
        pipeline::Pipeline,
        proc_macro2,
        quote::{quote, ToTokens},
        render::Diagnose,
    };
    use syn::{parse2, Attribute, DeriveInput, Field, ImplItem, ItemImpl};

    // --- a child stage, wholly derived ---
    #[derive(Extractor, Validate, Processor)]
    #[source(Field)]
    pub struct Col<'ast> {
        #[from(source.attrs.iter())]
        attrs: Vec<Extracted<Marker<'ast>, &'ast Attribute>>,
    }

    /// A leaf, hand-written: it has no fields to read the answer off.
    pub struct Marker<'ast>(core::marker::PhantomData<&'ast ()>);

    impl<'ast> Validate<'ast> for Marker<'ast> {
        type Source = &'ast Attribute;
        type Valid = &'ast Attribute;
        fn validate(i: Self::Source) -> Result<Self::Valid, Reason> { Ok(i) }
    }
    impl<'ast> Extractor<'ast> for Marker<'ast> {
        type Output = Extracted<Self, &'ast Attribute>;
        fn extract_from(node: &'ast Attribute) -> Self::Output {
            Extracted::new(Extraction::value(Marker(core::marker::PhantomData)), node)
        }
    }
    impl Assert for Marker<'_> {}
    impl Diagnose for Marker<'_> { fn diagnose(&self, _: &mut Vec<syn::Error>) {} }

    // --- the root, with a REAL validate written by hand ---
    pub struct Table<'ast> {
        cols: Vec<Extracted<Col<'ast>, &'ast Field>>,
    }

    impl<'ast> Validate<'ast> for Table<'ast> {
        type Source = &'ast DeriveInput;
        type Valid = &'ast syn::FieldsNamed;
        fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
            let syn::Data::Struct(d) = &input.data else {
                return Err(Reason::at(ReasonKind::WrongShape, &input.ident));
            };
            match &d.fields {
                syn::Fields::Named(n) => Ok(n),
                _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
            }
        }
    }
    impl<'ast> Extractor<'ast> for Table<'ast> {
        type Output = Extracted<Self, &'ast DeriveInput>;
        fn extract_from(node: &'ast DeriveInput) -> Self::Output {
            let e = match Self::validate(node) {
                Ok(named) => Extraction::value(Table { cols: Col::extract_each(named.named.iter()) }),
                Err(reason) => Extraction::failed(reason),
            };
            Extracted::new(e, node)
        }
    }
    impl Assert for Table<'_> {}
    impl Diagnose for Table<'_> {
        fn diagnose(&self, out: &mut Vec<syn::Error>) { self.cols.diagnose(out); }
    }

    // --- processing: real work, so hand-written ---
    pub struct Counted<'ast> { item: &'ast syn::Ident, cols: usize }

    impl<'ast> proc_macro_flow_traits::processor::Processor<'ast> for Table<'ast> {
        type Input = Extracted<Table<'ast>, &'ast DeriveInput>;
        type Output = Counted<'ast>;
        fn process(input: Self::Input) -> Extraction<Self::Output> {
            let item = &input.source().ident;
            match input.into_extraction().value {
                Some(t) => Extraction::value(Counted { item, cols: t.cols.len() }),
                None => Extraction::default(),
            }
        }
    }

    // --- generation: composed from children, via the derive ---
    pub struct Count(ImplItem);
    impl ToTokens for Count {
        fn to_tokens(&self, t: &mut proc_macro2::TokenStream) { self.0.to_tokens(t) }
    }
    impl<'ast> Generator<'ast> for Count {
        type Input = usize;
        type Subject = ();
        type Output = Self;
        fn generate(n: usize) -> Extraction<Self> {
            match parse2(quote!(pub const COLUMNS: usize = #n;)) {
                Ok(i) => Extraction::value(Count(i)),
                Err(e) => Extraction::failed(Reason::new(ReasonKind::Internal(e))),
            }
        }
        fn stub(_: ()) -> syn::Result<Self> {
            parse2(quote!(pub const COLUMNS: usize = 0;)).map(Count)
        }
    }

    #[derive(Generator)]
    #[builds(from = Counted<'ast>, subject = &'ast DeriveInput)]
    #[generates(count: Count = input.cols)]
    pub struct Block(ItemImpl);

    impl Block {
        fn assemble(input: &Counted<'_>, count: Count) -> syn::Result<Self> {
            let item = input.item;
            parse2(quote!(impl #item { #count })).map(Block)
        }
        fn assemble_stub(subject: &DeriveInput, count: Count) -> syn::Result<Self> {
            let item = &subject.ident;
            parse2(quote!(impl #item { #count })).map(Block)
        }
    }

    // --- wiring: by hand ---
    pub struct ByHand;
    impl<'ast> Pipeline<'ast> for ByHand {
        type Extractor = Table<'ast>;
        type Processor = Table<'ast>;
        type Generator = Block;
    }

    // --- wiring: generated, with the entry left to the author ---
    #[pipeline(derive = Wired, entry = manual)]
    mod wired {
        use super::*;
        #[extractor(source = ::proc_macro_flow_traits::syn::DeriveInput)]
        #[processor(from = Table)]
        pub type Table2<'ast> = Table<'ast>;
        // The lifetime is UNUSED here and must still be written: the generated wiring names every
        // stage as `module::Name<'ast>`, and a generator leaf that wraps a syn item borrows
        // nothing. An alias may carry a lifetime it does not use (VERIFIED), which is what lets a
        // lifetime-free stage be wired without giving it one it has no reason to have.
        #[generator(from = Table2)]
        pub type Block2<'ast> = Block;
    }

    #[test]
    fn running_it() {
        let input: DeriveInput = syn::parse_str("struct T { a: u8, b: u8 }").unwrap();
        let out = ByHand::run(&input).to_token_stream().to_string();
        assert!(out.contains("COLUMNS : usize = 2usize"), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn a_rejected_shape_still_emits_the_stub_beside_the_error() {
        let input: DeriveInput = syn::parse_str("enum E { A }").unwrap();
        let out = ByHand::run(&input).to_token_stream().to_string();
        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("COLUMNS"), "the stub is missing: {out}");
    }

    #[test]
    fn an_attribute_macros_source_carries_both_halves() {
        let args: syn::Meta = syn::parse_str("trace(level = \"debug\")").unwrap();
        let item: syn::ItemFn = syn::parse_str("fn f() {}").unwrap();
        let node = Attributed::new(&args, &item);
        assert_eq!(Annotated::item(node).sig.ident, "f");
    }
}
