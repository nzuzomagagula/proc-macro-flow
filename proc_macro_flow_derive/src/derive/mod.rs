// @review [ ]
//! The derives: generating extraction logic instead of writing it out.
//!
//! NOTE(#derive/cannot-self-host): V[N(proc_macro_flow_derive).!uses(Attr(derive(Extractor)))],
//! "VERIFIED: `can't use a procedural macro from the same crate that defines it`. So
//! StructExtraction and FieldExtraction - which live here - can NEVER carry these derives, and the
//! plan's 'rewrite the existing extractors onto the derive' step is not merely unfinished but
//! impossible where they sit. They stay hand-written as the bootstrap, and the derive is proved
//! from proc_macro_flow instead: the facade depends on both crates, so it can host the equivalent
//! extraction and assert the generated tree matches the hand-written one. Same proof, a crate over"
//!
//! NOTE(#derive/three-not-one): V[Attr(derive(Extractor)) != Attr(derive(Validate))], "Three
//! separate derives rather than one that emits everything, because the cases genuinely differ.
//! StructExtraction has a REAL validate - DeriveInput narrowed to &DataStruct - so a derive that
//! always emitted a trivial one would be unusable for exactly the type that motivated the design.
//! Deriving what you want generated and hand-writing the rest is ordinary Rust and needs no opt-out
//! attribute, which is the same reasoning ID(processor/optionality) already settled"

// TODO[x](#derive/structure-mirrors-the-pattern): M[N(derive).F(flat) => N(derive).N(stage)],
// "Each derive BECAME a pipeline in the conversion and stayed a single flat file, so the shape is
// true of the code and invisible in the tree. N(base/extractor) already shows what it should look
// like - a directory whose mod.rs names extractor, processor, generator and pipeline - and a
// reader opening N(derive/syntax) should find the same three stages rather than 800 lines to scroll.
//
// The structure is the cheapest documentation there is: it is read before any comment, it cannot
// disagree with itself, and a stage that has nowhere to live is a stage somebody skipped"
pub(crate) mod ext;
mod diagnose;
mod extractor;
mod generator;
mod processor;
mod stage;
mod syntax;
mod validate;

pub(crate) use diagnose::pipeline::DiagnoseWiring;
pub(crate) use extractor::pipeline::ExtractorWiring;
pub(crate) use generator::pipeline::GeneratorWiring;
pub(crate) use processor::pipeline::ProcessorWiring;
pub(crate) use syntax::pipeline::SyntaxWiring;
pub(crate) use validate::pipeline::ValidateWiring;

use ext::TypeExt;
use syn::{DeriveInput, Result, Type};

/// The lifetime a stage impl is written against, and the generics to declare it with.
///
/// NOTE(#derive/lifetime-is-introduced-when-absent): V[F(stage_lifetime).introduces], "Every stage
/// trait carries 'ast (NOTE(#pipeline/one-shape-per-stage)), but not every stage TYPE needs one - a
/// generator leaf wrapping a `syn::ImplItem` borrows nothing. So the derive uses the type's own
/// lifetime when it has one and INTRODUCES `'ast` when it does not, which is legal because the
/// trait reference constrains it. Requiring authors to declare a lifetime they never use would be
/// the derive making its own convenience their problem"
pub(crate) fn stage_lifetime(input: &DeriveInput) -> (syn::Generics, syn::Lifetime) {
    match input.generics.lifetimes().next() {
        Some(def) => (input.generics.clone(), def.lifetime.clone()),
        None => {
            let lifetime = syn::Lifetime::new("'ast", proc_macro2::Span::call_site());
            // A real Ty(Generics) rather than the tokens that would print as one - so the caller
            // splits it the same way it splits any other, and nothing malformed can be spliced.
            let mut generics = input.generics.clone();
            generics.params.insert(
                0,
                syn::GenericParam::Lifetime(syn::LifetimeParam::new(lifetime.clone())),
            );
            (generics, lifetime)
        }
    }
}

/// How many children a field declares, read off its written type.
///
/// This is `#from/arity-from-type`, and it is where a proc macro beats `macro_rules!`: the type is
/// *parsed*, so `std::option::Option<T>` and `Option<T>` are the same thing here, where a
/// declarative macro could only match the tokens it was handed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Arity {
    /// `Extracted<T, I>` — exactly one.
    One,
    /// `Vec<Extracted<T, I>>` — many.
    Many,
    /// `Option<Extracted<T, I>>` — absence is not a failure.
    Maybe,
}

/// A field's arity and the extractor that produces its children.
pub(crate) struct Child {
    pub(crate) arity: Arity,
    /// The `T` of `Extracted<T, I>`, which the generated call must name — `T::Output` is an
    /// associated type and so not inferable (`#from/names-its-target`).
    pub(crate) extractor: Type,
}

impl Child {
    pub(crate) fn of(ty: &Type) -> Result<Self> {
        if let Some(inner) = ty.unwrap_generic("Vec") {
            return Ok(Child {
                arity: Arity::Many,
                extractor: inner.extractor()?,
            });
        }
        if let Some(inner) = ty.unwrap_generic("Option") {
            return Ok(Child {
                arity: Arity::Maybe,
                extractor: inner.extractor()?,
            });
        }
        Ok(Child {
            arity: Arity::One,
            extractor: ty.extractor()?,
        })
    }
}
