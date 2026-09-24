// @review [ ]
//! What this derive reads: the shared stage declaration.
//!
//! The same re-export `#[derive(Validate)]` makes, for the same reason - see
//! ID(derive/shared-stages-are-re-exported). These two derives read one declaration and part
//! company at the generator, which is what ID(stage/one-reader-two-generators) describes.

pub(crate) use super::super::stage::StageDeclaration;
