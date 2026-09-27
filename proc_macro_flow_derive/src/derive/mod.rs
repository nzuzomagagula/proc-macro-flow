// @review [ ]
//! The derives: generating extraction logic instead of writing it out.
//!
// NOTE(#derive/cannot-self-host): V[N(proc_macro_flow_derive) != uses(Attr(derive(Extractor)))], "The derive crate cannot use its own derives"
//
// VERIFIED: `can't use a procedural macro from the same crate that defines it`. So
// StructExtraction and FieldExtraction - which live here - can NEVER carry these derives, and the
// plan's 'rewrite the existing extractors onto the derive' step is not merely unfinished but
// impossible where they sit. They stay hand-written as the bootstrap, and the derive is proved
// from proc_macro_flow instead: the facade depends on both crates, so it can host the equivalent
// extraction and assert the generated tree matches the hand-written one. Same proof, a crate over
//!
// NOTE(#derive/three-not-one): V[Attr(derive(Extractor)) != Attr(derive(Validate))], "Three derives, because the cases genuinely differ"
// Three separate derives rather than one that emits everything, because the cases genuinely differ.
// StructExtraction has a REAL validate - DeriveInput narrowed to &DataStruct - so a derive that
// always emitted a trivial one would be unusable for exactly the type that motivated the design.
// Deriving what you want generated and hand-writing the rest is ordinary Rust and needs no opt-out
// attribute, which is the same reasoning ID(processor/optionality) already settled

// TODO[x](#derive/structure-mirrors-the-pattern): M[N(derive).F(flat) => N(derive).N(stage)], "The derive module mirrors the extract/process/generate pattern"
//
// Each derive BECAME a pipeline in the conversion and stayed a single flat file, so the shape is
// true of the code and invisible in the tree. N(base/extractor) already shows what it should look
// like - a directory whose mod.rs names extractor, processor, generator and pipeline - and a
// reader opening N(derive/syntax) should find the same three stages rather than 800 lines to scroll.
//
// The structure is the cheapest documentation there is: it is read before any comment, it cannot
// disagree with itself, and a stage that has nowhere to live is a stage somebody skipped
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
