// @review [ ]
//! `#[derive(Syntax)]` — a grammar type declares itself.
//!
//! This is the bootstrap ID(syntax/derive) named: the derive is what lets a grammar be written as
//! ordinary Rust types instead of as a hand-rolled `meta_list!` invocation, and it is what puts the
//! whole vocabulary suite on the macro path for the first time.
//!
// NOTE(#syntax-derive/parses-the-type): V[F(process).has(F(Child::of))], "Arity is read by parsing the field's type"
// Arity is read by PARSING the field's type and looking at `segments.last()`, reusing the extractor
// derive's reader rather than writing a second one. That is the difference a proc macro makes and
// the reason this exists at all: M(meta_list) matches the TOKENS `Option < .. >`, so
// `std::option::Option<T>` reads as required there (ID(forwarding/no-option) covers why that stays
// loud). Here it is simply correct, because the type is parsed and a path's last segment is a
// question the AST can answer

pub mod extractor;
pub mod generator;
pub mod pipeline;
pub mod processor;
