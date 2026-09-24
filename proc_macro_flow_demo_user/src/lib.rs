// @review [ ]
// TODO[x](#user/the-chain-ends-here): C[N(demo_user)], "Nothing downstream of the macro author was
// ever compiled, so every claim about what a USER needs was untested. This crate depends on the
// macro and nothing else, which makes its Cargo.toml the assertion"
//! The furthest user: somebody who writes `#[derive(Columns)]` and never reads this framework.
//!
//! NOTE(#user/the-dependency-list-is-the-proof): V[N(user).deps == [demo]], "This crate's whole
//! claim is in its Cargo.toml. It depends on the MACRO and nothing else - not on syn, not on
//! quote, not on proc_macro_flow. Anything the generated code names has to resolve from here, so
//! a single bare `::syn::` path anywhere in the chain fails the build of this crate and no other.
//!
//! That is not hypothetical. The bare-path bug was shipped four times and every existing test was
//! blind to it, because the facade carries syn as a dev-dependency and every test module imports
//! what it needs directly. ID(hygiene/generated-paths-are-checked) is the text scan that catches
//! it early; THIS is the compile that cannot be fooled."
//!
//! The chain, end to end:
//!
//! ```text
//!   proc_macro_flow_traits      the traits and the vocabulary
//!   proc_macro_flow_derive      the derives and #[pipeline]
//!   proc_macro_flow_demo        a macro author: declares three stages, writes no entry point
//!   proc_macro_flow_demo_user   <- you are here: writes #[derive(Columns)] and gets on with it
//! ```

use proc_macro_flow_demo::Columns;

/// A row in a table, the way somebody who has never heard of an extractor would write one.
#[derive(Columns)]
pub struct User {
    #[column(rename = "user_id", key)]
    pub id: u64,
    pub email: String,
    #[column(skip)]
    pub cache: Vec<u8>,
}

/// No attributes at all: every field is a column under its own name.
#[derive(Columns)]
pub struct Plain {
    pub alpha: u8,
    pub beta: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_column_takes_its_fields_name_by_default() {
        assert_eq!(Plain::COLUMNS, [("alpha", false), ("beta", false)]);
    }

    #[test]
    fn rename_and_key_are_read_from_the_attribute() {
        assert_eq!(User::COLUMNS[0], ("user_id", true));
        assert_eq!(User::COLUMNS[1], ("email", false));
    }

    #[test]
    fn a_skipped_field_is_not_a_column() {
        assert!(!User::COLUMNS.iter().any(|(name, _)| *name == "cache"));
        assert_eq!(User::COLUMNS.len(), 2);
    }

    #[test]
    fn the_fields_themselves_are_untouched() {
        // A derive ADDS beside the item; it does not rewrite it. The struct is still the struct
        // that was written, `#[column(..)]` and all.
        let user = User { id: 1, email: "a@b.c".into(), cache: vec![] };
        assert_eq!(user.id, 1);
        assert!(user.cache.is_empty());
    }
}
