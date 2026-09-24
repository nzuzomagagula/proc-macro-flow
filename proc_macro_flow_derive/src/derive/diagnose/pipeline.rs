// @review [ ]
//! `#[derive(Diagnose)]`'s macro boundary.

use proc_macro_flow_traits::pipeline::Pipeline;

use super::extractor::WalkDeclaration;
use super::generator::WalkExpansion;

/// `#[derive(Diagnose)]`, wired.
pub(crate) struct DiagnoseWiring;

impl<'ast> Pipeline<'ast> for DiagnoseWiring {
    type Extractor = WalkDeclaration<'ast>;
    type Processor = WalkDeclaration<'ast>;
    type Generator = WalkExpansion;
}

#[cfg(test)]
mod tests {
    use super::DiagnoseWiring;
    use proc_macro_flow_traits::pipeline::Pipeline;
    use quote::ToTokens;
    use syn::{parse_str, DeriveInput};

    fn expand(source: &str) -> String {
        let input: DeriveInput = parse_str(source).expect("the item parses");
        DiagnoseWiring::run(&input).to_token_stream().to_string()
    }

    #[test]
    fn both_impls_come_out_and_the_type_says_two() {
        let out = expand("struct S<'ast> { kids: Vec<Extracted<C<'ast>, &'ast I>> }");

        assert!(out.contains("Diagnose for S"), "{out}");
        assert!(out.contains("Assert for S"), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn every_field_is_walked_unless_it_says_otherwise() {
        // ID(diagnose-derive/walk-all-and-skip). The default is walk, so the failure that
        // compiles - a silently unreachable subtree - cannot be reached by forgetting anything.
        let out = expand(
            "struct S<'ast> { a: Vec<Extracted<C<'ast>, &'ast I>>, #[skip] name: &'ast Ident }",
        );

        assert!(out.contains("diagnose (& self . a"), "{out}");
        assert!(!out.contains("diagnose (& self . name"), "a skipped field was walked: {out}");
    }

    #[test]
    fn every_walked_field_is_also_asked_for_its_rules() {
        // ID(diagnose-derive/asks-what-it-walks). A grammar held as a VALUE states its rules
        // through Tr(Assert), not the walk - so an Assert that does not descend drops them, which
        // is how the demo's `conflicts(skip, key)` never fired.
        let out = expand("struct S { column: Option<Column>, #[skip] name: Option<Ident> }");

        assert!(out.contains("assert (& self . column"), "{out}");
        assert!(!out.contains("assert (& self . name"), "a skipped field was asked: {out}");
    }

    #[test]
    fn a_type_with_no_fields_walks_nothing_and_still_gets_both_impls() {
        let out = expand("struct S {}");

        assert!(out.contains("Diagnose for S"), "{out}");
        assert!(out.contains("Assert for S"), "{out}");
    }

    #[test]
    fn the_types_own_generics_are_used_unchanged() {
        // Tr(Diagnose) carries no lifetime of its own, so unlike the stage traits there is nothing
        // to introduce for a type that declares none.
        let out = expand("struct S<'ast, T> { a: Vec<Extracted<T, &'ast I>> }");

        assert!(out.contains("impl < 'ast , T >"), "{out}");
        assert!(out.contains("for S < 'ast , T >"), "{out}");
    }

    #[test]
    fn an_enum_is_reported_rather_than_guessed_at() {
        let out = expand("enum E { A }");

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("named fields"), "{out}");
    }
}
