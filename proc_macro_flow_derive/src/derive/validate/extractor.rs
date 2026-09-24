// @review [ ]
//! What this derive reads: the shared stage declaration.
//!
//! NOTE(#derive/shared-stages-are-re-exported): V[N(validate/extractor).R(N(stage))], "A module
//! holding one `pub use` rather than a type, and that is the honest thing for it to hold.
//! Attr(derive(Validate)) and Attr(derive(Processor)) read the SAME declaration - `#[source(Ty)]`
//! plus `#[args(Ty)]` - so duplicating the reader to give each its own file would be two
//! definitions of one fact, which is the drift the structure exists to prevent.
//!
//! What the module says instead is which type fills the role, which is exactly what
//! `type Extractor = ..` says in the wiring beside it. The alternative - leaving the file out - would
//! make this derive look like it has no extractor at all."

pub(crate) use super::super::stage::StageDeclaration;
