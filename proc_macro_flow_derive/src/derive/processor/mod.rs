// @review [ ]
//! `#[derive(Processor)]` — the identity, as a pipeline.
//!
//! Most extractions copy their fields through and generate from them with no transformation, so
//! the default has to cost nothing. Deriving this IS the opt-in: a type needing real processing
//! omits the derive and writes `impl Processor` by hand, which is ordinary Rust and needs no
//! opt-out attribute.
//!
//! It shares its extractor and processor with `#[derive(Validate)]` and differs only at the
//! generator - see NOTE(#stage/one-reader-two-generators).

pub mod extractor;
pub mod generator;
pub mod pipeline;
pub mod processor;
