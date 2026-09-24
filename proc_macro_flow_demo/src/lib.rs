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
// TODO[x](#demo/boilerplate-absorbed): U[N(demo).lines], "99 non-comment lines of which ~14 were
// the author's own logic: a twin struct with a field-for-field copy between them, a hand-forwarded
// ToTokens, and two EMPTY impls. Only *how* belongs to the author - what the shape must be is
// declared, and everything between is the macro's"
#[pipeline(derive = Generated)]
mod generated {
    use proc_macro_flow_derive::{Extractor, Generator, Processor};
    use proc_macro_flow_traits::{
        extractor::{Reason, ReasonKind, Validate},
        quote::quote,
        syn::{self, parse2, Data, DeriveInput, FieldsNamed, Ident, ItemImpl},
    };

    /// Reads the struct's field names, and is its own processor - one type, two roles, which is
    /// ID(pipeline/no-processor-is-the-extractor) and the commonest shape there is.
    ///
    /// Attr(derive(Extractor)) writes Tr(Extractor), Tr(Diagnose) and Tr(Assert); Attr(derive(Processor))
    /// writes the identity. Neither field is a child extraction, so both are Attr(value) - see
    /// NOTE(#extractor-derive/children-are-marked-not-inferred).
    #[derive(Extractor, Processor)]
    #[source(DeriveInput)]
    // FULLY QUALIFIED, and it must be: this type is spliced into the entry function, which
    // Attr(pipeline) emits as a SIBLING of this module and therefore OUTSIDE it - so the `use`
    // above is not in scope there. Naming a path rather than an ident is what
    // ID(pipeline-macro/values-are-types) made possible.
    #[extractor(source = ::proc_macro_flow_traits::syn::DeriveInput)]
    #[processor(from = Read)]
    pub struct Read<'ast> {
        #[value(source.0)]
        pub item: &'ast Ident,
        #[value(source.1.named.iter().filter_map(|f| f.ident.as_ref()).collect())]
        pub fields: Vec<&'ast Ident>,
    }

    /// The only hand-written stage, and the only one that should be.
    ///
    /// NOTE(#demo/valid-is-what-the-stage-needs): V[Ty(Valid).pair], "A narrowing is a DECISION, so
    /// Attr(derive(Validate)) declines to guess at one and this is written out. What it narrows to
    /// is a PAIR rather than the `&FieldsNamed` the shape check produces, because every Attr(value)
    /// below is written against `source`: narrowing to the fields alone would put the type's own
    /// name out of reach. Ty(Valid) is exactly the place to say what this stage needs."
    impl<'ast> Validate<'ast> for Read<'ast> {
        type Source = &'ast DeriveInput;
        type Valid = (&'ast Ident, &'ast FieldsNamed);

        fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
            let Data::Struct(data) = &input.data else {
                return Err(Reason::at(ReasonKind::WrongShape, &input.ident));
            };

            match &data.fields {
                syn::Fields::Named(named) => Ok((&input.ident, named)),
                _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
            }
        }
    }

    /// One impl, and the type says one - ID(generation/newtype-per-item). Attr(derive(Generator))
    /// writes Tr(Generator) and the Tr(ToTokens) that lowers it; the two assemble functions are the
    /// only thing it cannot know (NOTE(#generator-derive/plumbing-not-logic)).
    #[derive(Generator)]
    #[builds(from = Read<'ast>, subject = &'ast DeriveInput)]
    pub struct Block(ItemImpl);

    /// The generator ROLE, on an alias.
    ///
    /// The generated wiring names every stage `module::Name<'ast>`, and a generator leaf wrapping a
    /// syn item borrows nothing - so S(Block) has no lifetime to give it. An alias may carry one it
    /// does not use, which is the documented way to wire a lifetime-free stage without inventing a
    /// lifetime for it.
    #[generator(from = Read)]
    pub type Built<'ast> = Block;

    impl Block {
        fn assemble(input: &Read<'_>) -> syn::Result<Self> {
            let item = input.item;
            let names = input.fields.iter().map(|ident| ident.to_string());

            parse2(quote! {
                impl #item {
                    /// Every field's name, in declaration order.
                    pub const FIELD_NAMES: &'static [&'static str] = &[ #(#names),* ];
                }
            })
            .map(Block)
        }

        /// The vacant form is the same SHAPE, so a failure does not cascade into
        /// "no associated item" at every use site - ID(generator/stub-is-not-empty).
        fn assemble_stub(subject: &DeriveInput) -> syn::Result<Self> {
            let item = &subject.ident;
            parse2(quote! {
                impl #item {
                    pub const FIELD_NAMES: &'static [&'static str] = &[];
                }
            })
            .map(Block)
        }
    }
}
