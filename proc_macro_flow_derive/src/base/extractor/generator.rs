// @review [ ]
//! The concrete generator for the extractor stage, as a PARENT and two LEAVES.
//!
//! NOTE(#generator/base-scope): a generator emits the FULL body when there is a value and a STUB of
//! the same type when there is not. Both are the same item from a use site's view, which is all
//! ID(generator/stub-is-not-empty) needs.
//!
//! NOTE(#generator/newtypes-here): V[S(Block) && S(Fields) && S(Shapes)], "The first real use of
//! ID(generation/newtype-per-item). What used to be one generator returning a bare ItemImpl is a
//! parent and two leaves, each its own TYPE - which is what makes a failure attributable: the thing
//! that failed has a name, because it IS a name.
//!
//! The leaves take OWNED inputs (`Vec<String>`) rather than borrowing the parent's processed value,
//! and that is not laziness. Tr(Generator) carries no lifetime parameter, so an Ty(Input) that
//! borrows would force every newtype to carry 'ast through PhantomData just to name it. Owned
//! inputs also state the rule ID(generation/parent-feeds-children) means literally: a child gets
//! exactly what it needs and cannot reach for anything else"

use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use quote::{quote, ToTokens};
use syn::{parse2, DeriveInput, ImplItem, ItemImpl};

use crate::base::extractor::processor::ProcessedStruct;
use crate::base::syntax::extractor::SyntaxHelper;

// TODO(#generator/macro):C[F(generator)], "Proc-macro entry point for the generator stage, alongside lib.rs::field_names (ID(extractor/macro-wiring))"

/// The impl block the whole pipeline produces.
pub(crate) struct Block(pub(crate) ItemImpl);

/// `const FIELDS` — one item, built from every field's name.
pub(crate) struct Fields(pub(crate) ImplItem);

/// `const SHAPES` — one item, built from every field's shape tokens.
pub(crate) struct Shapes(pub(crate) ImplItem);

impl ToTokens for Block {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
    }
}

impl ToTokens for Fields {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
    }
}

impl ToTokens for Shapes {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
    }
}

/// Build one item, or say why not — the shape every leaf shares.
///
/// `Internal` and not `Syntax`: these tokens are ours, assembled from names we already hold, so a
/// parse failure here is a FRAMEWORK bug. NOTE(#reason/fault-is-declared) is why that is declared
/// at the raise site rather than guessed.
fn leaf<T>(tokens: proc_macro2::TokenStream, wrap: fn(ImplItem) -> T) -> Extraction<T> {
    match parse2::<ImplItem>(tokens) {
        Ok(item) => Extraction::value(wrap(item)),
        Err(error) => Extraction::failed(Reason::new(ReasonKind::Internal(error))),
    }
}

impl Generator for Fields {
    /// The field names, already reduced. See NOTE(#generator/newtypes-here).
    type Input = Vec<String>;
    type Subject = ();
    type Output = Self;

    fn generate(names: Vec<String>) -> Extraction<Self> {
        leaf(
            quote!( pub const FIELDS: &'static [&'static str] = &[ #(#names),* ]; ),
            Fields,
        )
    }

    fn stub(_: ()) -> syn::Result<Self> {
        parse2(quote!( pub const FIELDS: &'static [&'static str] = &[]; )).map(Fields)
    }
}

impl Generator for Shapes {
    type Input = Vec<String>;
    type Subject = ();
    type Output = Self;

    fn generate(shapes: Vec<String>) -> Extraction<Self> {
        leaf(
            quote!( pub const SHAPES: &'static [&'static str] = &[ #(#shapes),* ]; ),
            Shapes,
        )
    }

    fn stub(_: ()) -> syn::Result<Self> {
        parse2(quote!( pub const SHAPES: &'static [&'static str] = &[]; )).map(Shapes)
    }
}

impl<'ast> Generator for ProcessedStruct<'ast> {
    type Input = Self;
    type Subject = &'ast DeriveInput;
    type Output = Block;

    fn generate(input: Self) -> Extraction<Block> {
        let mut out: Extraction<Block> = Extraction::default();

        // The parent feeds each child exactly what it needs, and nothing else.
        let names = input
            .fields
            .iter()
            .enumerate()
            .map(|(index, processed)| match &processed.field.ident {
                // Unnamed fields are addressed positionally, which is what a tuple struct's
                // "name" is.
                Some(ident) => ident.to_string(),
                None => index.to_string(),
            })
            .collect();

        let shapes = input
            .fields
            .iter()
            .map(|processed| {
                processed
                    .attrs
                    .iter()
                    .find(|attribute| attribute.helper == SyntaxHelper::Shape)
                    .map(|attribute| attribute.tokens.to_string())
                    .unwrap_or_default()
            })
            .collect();

        // PER-CHILD ISOLATION. A child that fails is STUBBED and its reason kept, so its siblings
        // still reach the output - which a Result could not express without dropping one of them.
        let fields = match out.absorb(Fields::generate(names)) {
            Some(item) => item,
            None => match Fields::stub(()) {
                Ok(vacant) => vacant,
                Err(error) => return fail(out, error),
            },
        };

        let shapes = match out.absorb(Shapes::generate(shapes)) {
            Some(item) => item,
            None => match Shapes::stub(()) {
                Ok(vacant) => vacant,
                Err(error) => return fail(out, error),
            },
        };

        let name = &input.item.ident;
        let (impl_generics, type_generics, where_clause) = input.item.generics.split_for_impl();

        match parse2::<ItemImpl>(quote! {
            impl #impl_generics #name #type_generics #where_clause {
                #fields
                #shapes
            }
        }) {
            Ok(item) => {
                out.value = Some(Block(item));
                out
            }
            Err(error) => fail(out, error),
        }
    }

    fn stub(subject: &'ast DeriveInput) -> syn::Result<Block> {
        let name = &subject.ident;
        let (impl_generics, type_generics, where_clause) = subject.generics.split_for_impl();
        let fields = Fields::stub(())?;
        let shapes = Shapes::stub(())?;

        parse2(quote! {
            impl #impl_generics #name #type_generics #where_clause {
                #fields
                #shapes
            }
        })
        .map(Block)
    }
}

/// Record a framework failure and hand back a valueless extraction.
fn fail(mut out: Extraction<Block>, error: syn::Error) -> Extraction<Block> {
    out.reasons.push(Reason::new(ReasonKind::Internal(error)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro_flow_traits::pipeline::Pipeline;
    use syn::parse_str;

    use crate::base::extractor::pipeline::ExtractorPipeline;

    /// The whole pipeline, exactly as `lib.rs::field_names` runs it.
    fn pipeline(source: &str) -> String {
        let item: &'static DeriveInput =
            Box::leak(Box::new(parse_str(source).expect("the item parses")));

        ExtractorPipeline::run(item).to_string()
    }

    #[test]
    fn a_struct_generates_its_field_names() {
        let out = pipeline("pub struct Thing { a: u8, b: String }");

        assert!(out.contains("impl Thing"), "{out}");
        assert!(out.contains(r#""a""#), "{out}");
        assert!(out.contains(r#""b""#), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn a_tuple_struct_names_its_fields_positionally() {
        let out = pipeline("pub struct Thing(u8, String);");
        assert!(out.contains(r#""0""#), "{out}");
        assert!(out.contains(r#""1""#), "{out}");
    }

    #[test]
    fn generics_are_carried_through() {
        let out = pipeline("pub struct Thing<T: Clone> { a: T }");
        assert!(out.contains("impl < T : Clone > Thing < T >"), "{out}");
    }

    #[test]
    fn a_failed_extraction_still_emits_the_impl() {
        // An enum is rejected, so there is no value - and the impl must exist anyway, or every use
        // site reports 'no associated item named FIELDS' on top of the real error.
        let out = pipeline("pub enum Thing { A, B }");

        assert!(out.contains("impl Thing"), "the stub is missing: {out}");
        assert!(out.contains("FIELDS"), "the stub is vacant of items: {out}");
        assert!(out.contains("compile_error"), "the reason is missing: {out}");
    }

    #[test]
    fn the_stub_precedes_the_error() {
        let out = pipeline("pub enum Thing { A, B }");
        assert!(out.find("impl").unwrap() < out.find("compile_error").unwrap(), "{out}");
    }

    #[test]
    fn a_shape_attribute_reaches_generation() {
        let out = pipeline("pub struct Thing { #[shape(AttributeKind::MetaList)] a: u8 }");

        assert!(out.contains("SHAPES"), "{out}");
        assert!(out.contains("AttributeKind :: MetaList"), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn a_doc_comment_does_not_become_a_shape_or_an_error() {
        let out = pipeline("pub struct Thing { /// documented\n a: u8 }");
        assert!(!out.contains("compile_error"), "{out}");
    }

    // ---- per-child isolation: the reason the newtypes exist -------------------------------

    /// One field, so the leaves have something to build from.
    fn processed(source: &str) -> ProcessedStruct<'static> {
        use proc_macro_flow_traits::{extractor::Extractor, processor::Processor};
        use crate::base::extractor::extractor::StructExtraction;

        let item: &'static DeriveInput =
            Box::leak(Box::new(parse_str(source).expect("the item parses")));
        StructExtraction::process(StructExtraction::extract_from(item))
            .value
            .expect("a struct processes")
    }

    #[test]
    fn a_leaf_cannot_fail_on_its_typed_input() {
        // Worth asserting because it is the design working rather than a gap in the tests: given
        // `Vec<String>`, EVERY value interpolates as a valid string literal, so there is no input
        // Fields can be handed that it cannot build. Typing the input removed the failure mode.
        let hostile = Fields::generate(vec![
            "not an ident \" oops".into(),
            String::new(),
            "}{".into(),
        ]);

        assert!(hostile.value.is_some(), "typed input should be unfailable");
        assert!(hostile.reasons.is_empty());
    }

    #[test]
    fn a_malformed_leaf_is_our_fault_and_says_so() {
        // The shared failure path, exercised directly - nothing in the pipeline can currently
        // reach it, which is why it is tested here rather than through `pipeline()`.
        let broken = leaf(quote!(this is not an impl item), Fields);

        assert!(broken.value.is_none(), "the leaf should have failed");
        assert_eq!(broken.reasons.len(), 1);
        assert!(
            broken.reasons[0].is_internal(),
            "tokens WE assembled are our fault, never the author's",
        );
    }

    // TODO[ ](#generator/inject-a-failing-child):V[test.parent_isolation], "The plan's per-child
    // isolation assertion - a parent whose SECOND child fails still emits the first - cannot be
    // written yet: S(ProcessedStruct) calls S(Fields) and S(Shapes) inline, so there is no seam to
    // inject a failure through, and adding one purely for a test would be worse than waiting. It
    // becomes writable the moment ID(generation/composition-derive) generates the composition,
    // because the children are then named by the attribute and a test can declare its own"

    #[test]
    fn a_leaf_stub_is_the_same_shape_as_its_success() {
        // Enforced by TYPE now - both arms are `Fields` - so this asserts the CONTENT rule that
        // the type cannot: a use site finds FIELDS either way.
        let full = Fields::generate(vec!["a".into()]).value.expect("well formed");
        let vacant = Fields::stub(()).expect("the stub is well formed");

        assert!(full.to_token_stream().to_string().contains("FIELDS"));
        assert!(vacant.to_token_stream().to_string().contains("FIELDS"));
    }

    #[test]
    fn the_parent_assembles_both_children() {
        let value = processed("pub struct Thing { a: u8, b: u8 }");
        let generated = ProcessedStruct::generate(value);

        assert!(generated.reasons.is_empty(), "nothing should have failed");
        let out = generated.value.expect("a block").to_token_stream().to_string();
        assert!(out.contains("FIELDS"), "{out}");
        assert!(out.contains("SHAPES"), "{out}");
    }
}
