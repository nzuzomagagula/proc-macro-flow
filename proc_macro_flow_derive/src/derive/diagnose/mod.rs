// @review [ ]
// TODO[x](#diagnose/derive-the-walk): C[MacDef(Diagnose)], "Fifteen stages carried an empty Assert and a field-listing walk"
// About fifteen hand-written stages each carried an `impl Assert for X {}` that states nothing and
// a Diagnose body that only lists fields. Neither is a decision anybody made. Added on a COUNT, not
// on taste - at two or three a derive would have been ceremony
//! `#[derive(Diagnose)]` — write the walk, and the `Assert` that asks the same fields.
//!
// NOTE(#diagnose-derive/why-a-seventh-derive): V[N(workspace).has(more than 12 empty Asserts)], "Added on a count, not on taste"
// Added on a COUNT rather than on taste. Attr(derive(Extractor)) already emits both impls for a
// stage it generates, so the question was how many stages are hand-written - and the answer across
// this workspace was about fifteen, each carrying an `impl Assert for X {}` that states nothing and
// a Tr(Diagnose) body that only lists fields.
//
// At two or three that is honest and a derive would be ceremony. At fifteen it is a tax, and the
// thing being typed is not a decision anybody made.

pub mod extractor;
pub mod generator;
pub mod pipeline;
pub mod processor;
