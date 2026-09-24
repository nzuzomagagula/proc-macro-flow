// @review [ ]
//! `#[derive(Extractor)]` — generate `extract_from` from `#[source(..)]` and `#[from]` / `#[with]`.
//!
// NOTE(#derive/diagnose-rides-along): V[F(generate).has(Impl(Diagnose))], "Diagnose rides along with derive(Extractor)"
// Tr(Diagnose) is emitted HERE rather than as a fourth derive, and that does not contradict
// ID(derive/three-not-one). Those three are separate because each has a real hand-written case:
// Validate narrows, Processor does work. Tr(Diagnose) has none - it answers only 'where are my
// children', and for a struct whose every field is a declared child the field list IS the answer,
// with no alternative an author could want instead. A derive that always emits the same correct
// thing should not be opt-in; making it one would just be a way to forget it, and a forgotten
// Tr(Diagnose) is a silently unreachable subtree rather than a compile error

pub mod extractor;
pub mod generator;
pub mod pipeline;
pub mod processor;
