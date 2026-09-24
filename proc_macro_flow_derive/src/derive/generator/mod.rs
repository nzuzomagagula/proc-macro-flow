// @review [ ]
//! `#[derive(Generator)]` — a parent declares its children and what it feeds each one.
//!
//! NOTE(#generator-derive/plumbing-not-logic): V[F(generate).emits(composition) && !emits(assemble)],
//! "The derive emits the COMPOSITION - call each child with its fed input, absorb the result, stub
//! the ones that failed - and requires the author to write F(assemble), which puts the children
//! into an item. That split is forced rather than chosen: the derive cannot know what SHAPE a
//! parent's item is. It knows there are two children; it cannot know they belong inside
//! `impl Thing { .. }` rather than a module or a match arm.
//!
//! Which is the same split ID(processor/derive-enforces) settles for processing, and it lands the
//! same way: a missing F(assemble) is a compile error, not a silently trivial generator"

pub mod extractor;
pub mod generator;
pub mod pipeline;
pub mod processor;
