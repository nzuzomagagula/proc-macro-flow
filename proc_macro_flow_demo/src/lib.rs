// @review [ ]
//! A pipeline written the way an AUTHOR would write one, so that the generated entry point is
//! compiled at least once.
//!
//! NOTE(#demo/why-a-third-crate): V[N(demo).proc_macro && N(demo) != N(derive)], "This crate exists
//! because two rustc constraints meet and leave nowhere else to stand.
//!
//! (1) `can't use a procedural macro from the same crate that defines it` - VERIFIED, and already
//! recorded as ID(derive/cannot-self-host). So proc_macro_flow_derive cannot apply its own
//! Attr(pipeline), which is what the plan's 'regenerate lib.rs::field_names with Attr(pipeline)'
//! asked for and why that step is impossible rather than merely unfinished.
//!
//! (2) `functions tagged with #[proc_macro_derive] must currently reside in the root of the crate`,
//! and only a `proc-macro = true` crate may have one at all. So the facade cannot host it either -
//! which is the one thing ID(facade/hosts-the-proof) could never cover.
//!
//! The gap that leaves is the one this closes. Every proof of Attr(pipeline) so far ran under
//! `entry = manual`, so the entry function it generates had been asserted as TOKENS and never once
//! handed to rustc. Here it is compiled, exported, and used by the facade against a real struct."

use proc_macro_flow_derive::pipeline;

// MUST BE AT THE CRATE ROOT. Attr(pipeline) emits the entry function as a SIBLING of this module,
// and (2) above is why that is the only place it can land - NOTE(#pipeline/entry-is-a-sibling).
#[pipeline(derive = Generated)]
mod generated {
    use proc_macro_flow_traits::{
        assert::Assert,
        extractor::{Extracted, Extraction, Extractor, Reason, ReasonKind, Validate},
        generator::Generator,
        processor::Processor,
        proc_macro2::TokenStream,
        quote::{quote, ToTokens},
        render::Diagnose,
        syn::{self, Data, DeriveInput, Ident, ItemImpl},
    };

    /// Reads the struct's field names, and is its own processor - one type, two roles, which is
    /// ID(pipeline/no-processor-is-the-extractor) and the commonest shape there is.
    // FULLY QUALIFIED, and it must be: this type is spliced into the entry function, which
    // Attr(pipeline) emits as a SIBLING of this module and therefore OUTSIDE it - so the `use`
    // above is not in scope there. Naming a path rather than an ident is what
    // ID(pipeline-macro/values-are-types) made possible.
    #[extractor(source = ::proc_macro_flow_traits::syn::DeriveInput)]
    #[processor(from = Read)]
    pub struct Read<'ast> {
        pub item: &'ast Ident,
        pub fields: Vec<&'ast Ident>,
    }

    /// What generation is written against.
    #[generator(from = Read)]
    pub struct Named<'ast> {
        pub item: &'ast Ident,
        pub fields: Vec<&'ast Ident>,
    }

    /// One impl, and the type says one - ID(generation/newtype-per-item).
    pub struct Block(ItemImpl);

    impl ToTokens for Block {
        fn to_tokens(&self, tokens: &mut TokenStream) {
            self.0.to_tokens(tokens);
        }
    }

    impl<'ast> Validate<'ast> for Read<'ast> {
        type Source = &'ast DeriveInput;
        type Valid = &'ast syn::FieldsNamed;

        fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
            let Data::Struct(data) = &input.data else {
                return Err(Reason::at(ReasonKind::WrongShape, &input.ident));
            };

            match &data.fields {
                syn::Fields::Named(named) => Ok(named),
                _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
            }
        }
    }

    impl<'ast> Extractor<'ast> for Read<'ast> {
        type Output = Extracted<Self, &'ast DeriveInput>;

        fn extract_from(node: &'ast DeriveInput) -> Self::Output {
            let extraction = match Self::validate(node) {
                Ok(named) => Extraction::value(Read {
                    item: &node.ident,
                    fields: named.named.iter().filter_map(|f| f.ident.as_ref()).collect(),
                }),
                Err(reason) => Extraction::failed(reason),
            };

            Extracted::new(extraction, node)
        }
    }

    impl Assert for Read<'_> {}

    impl Diagnose for Read<'_> {
        /// A leaf: its children are idents borrowed from the AST, not extractions.
        fn diagnose(&self, _: &mut Vec<syn::Error>) {}
    }

    impl<'ast> Processor<'ast> for Read<'ast> {
        type Input = Extracted<Read<'ast>, &'ast DeriveInput>;
        type Output = Named<'ast>;

        fn process(input: Self::Input) -> Extraction<Self::Output> {
            // NOTE(#processor/reasons-are-new-not-inherited): the walk has already rendered
            // whatever the extraction carried.
            match input.into_extraction().value {
                Some(read) => Extraction::value(Named {
                    item: read.item,
                    fields: read.fields,
                }),
                None => Extraction::default(),
            }
        }
    }

    impl<'ast> Generator<'ast> for Named<'ast> {
        type Input = Self;
        type Subject = &'ast DeriveInput;
        type Output = Block;

        fn generate(input: Self) -> Extraction<Block> {
            let item = input.item;
            let names = input.fields.iter().map(|ident| ident.to_string());

            match syn::parse2(quote! {
                impl #item {
                    /// Every field's name, in declaration order.
                    pub const FIELD_NAMES: &'static [&'static str] = &[ #(#names),* ];
                }
            }) {
                Ok(block) => Extraction::value(Block(block)),
                Err(error) => {
                    Extraction::failed(Reason::new(ReasonKind::Internal(error)))
                }
            }
        }

        /// The vacant form is the same SHAPE, so a failure does not cascade into
        /// "no associated item" at every use site - ID(generator/stub-is-not-empty).
        fn stub(subject: &'ast DeriveInput) -> syn::Result<Block> {
            let item = &subject.ident;
            syn::parse2(quote! {
                impl #item {
                    pub const FIELD_NAMES: &'static [&'static str] = &[];
                }
            })
            .map(Block)
        }
    }
}
