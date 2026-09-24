// @review [ ]
//! `#[derive(Validate)]` — the trivial pass-through, as a pipeline.
//!
//! Separate from `#[derive(Extractor)]` because a real narrowing validate is the interesting case:
//! `StructExtraction` turns a `DeriveInput` into a `&DataStruct`, and a derive that always emitted
//! a trivial one would be unusable for exactly the type that motivated the design. Derive this when
//! there is nothing to check; write it by hand when there is.
//!
//! NOTE(#derive/is-a-pipeline): V[Impl(Pipeline).for(ValidateWiring)], "The derive that generates
//! pipelines is now written as one. What that buys is not symmetry: F(run) owns the normalisation
//! every entry point used to repeat - walk for reasons before processing consumes the tree, choose
//! generate-or-stub, lower, append one compile_error per reason - so the entry function is a single
//! line and the stages hold only what is specific to them (ID(pipeline/owns-normalisation))."

pub mod extractor;
pub mod generator;
pub mod pipeline;
pub mod processor;
