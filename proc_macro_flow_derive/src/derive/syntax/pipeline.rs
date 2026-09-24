// @review [ ]
//! `#[derive(Syntax)]`'s macro boundary.

use proc_macro_flow_traits::pipeline::Pipeline;

use super::extractor::GrammarDeclaration;
use super::generator::SyntaxExpansion;

/// `#[derive(Syntax)]`, wired.
// TODO[x](#syntax/derive-is-a-pipeline): R[F(derive_syntax) -> S(SyntaxWiring)], "The largest
// split: extraction carries Attr(alias), Attr(shape) and the rule metas AS WRITTEN, and everything
// derived - heck's casings, the entry head, arity, and the three rule checks - is processing,
// which is the only stage that sees every field at once"
pub(crate) struct SyntaxWiring;

impl<'ast> Pipeline<'ast> for SyntaxWiring {
    type Extractor = GrammarDeclaration<'ast>;
    type Processor = GrammarDeclaration<'ast>;
    type Generator = SyntaxExpansion;
}

#[cfg(test)]
mod tests {
    use super::SyntaxWiring;
    use proc_macro_flow_traits::pipeline::Pipeline;
    use quote::ToTokens;
    use syn::{parse_str, DeriveInput};

    /// Drive the WHOLE pipeline, exactly as the entry function does.
    fn expand(source: &str) -> String {
        let input: DeriveInput = parse_str(source).expect("the item parses");
        SyntaxWiring::run(&input).to_token_stream().to_string()
    }

    /// The same, asserting the grammar was refused.
    ///
    /// A pipeline does not return an error - it emits one, beside whatever it could still build.
    /// So a rejection is read out of the OUTPUT now, which is also what the author sees.
    fn rejected(source: &str) -> String {
        let out = expand(source);
        assert!(out.contains("compile_error"), "the derive accepted this: {out}");
        out
    }

    #[test]
    fn a_grammar_with_no_rules_still_gets_an_assert_that_descends() {
        // The descent is what carries a NESTED grammar's rules up, so it is emitted whether or not
        // this type states any of its own.
        let out = expand("struct Retry { times: Option<LitInt> }");

        assert!(out.contains("Assert for Retry"), "{out}");
        assert!(out.contains("Assert :: assert (& self . times"), "{out}");
    }

    #[test]
    fn a_rule_becomes_a_check_against_the_arity_the_type_declared() {
        let out = expand(
            "#[assert(one_of(times, forever))] struct Retry { times: Option<LitInt>, forever: Option<LitBool> }",
        );

        assert!(out.contains("AssertKind :: OneOf"), "{out}");
        // Presence read off the TYPE - Option asks is_some, and nothing declared it.
        assert!(out.contains("Option :: is_some (& self . times)"), "{out}");
    }

    #[test]
    fn a_repeated_field_asks_whether_it_is_empty() {
        let out = expand("#[assert(any_of(a, b))] struct G { a: Vec<LitStr>, b: Vec<LitStr> }")
            ;

        assert!(out.contains("! :: std :: vec :: Vec :: is_empty (& self . a)"), "{out}");
    }

    #[test]
    fn a_rule_naming_a_field_that_is_not_there_is_rejected() {
        // ID(assert/rules-are-checked-at-derive-time), first check.
        let message = rejected(
            "#[assert(one_of(times, forevr))] struct Retry { times: Option<LitInt>, forever: Option<LitBool> }",
        );

        assert!(message.contains("no field named `forevr`"), "{message}");
        // And it says what there IS, which is the half that makes it actionable.
        assert!(message.contains("`times`"), "{message}");
        assert!(message.contains("`forever`"), "{message}");
    }

    #[test]
    fn a_rule_naming_a_required_field_is_rejected() {
        // The second check, and the one that earns this design: `times: LitInt` is ALWAYS written,
        // so asking whether it was is a statement its own type contradicts - ID(from/arity-from-type).
        let message = rejected(
            "#[assert(one_of(times, forever))] struct Retry { times: LitInt, forever: Option<LitBool> }",
        );

        assert!(message.contains("`times` is required"), "{message}");
        assert!(message.contains("Option"), "the fix must be named: {message}");
    }

    #[test]
    fn a_rule_given_the_wrong_number_of_keys_is_rejected() {
        // The third check. `requires` is a relation between exactly two keys.
        let three = rejected(
            "#[assert(requires(a, b, c))] struct G { a: Option<LitStr>, b: Option<LitStr>, c: Option<LitStr> }",
        );
        assert!(three.contains("exactly 2"), "{three}");

        let one = rejected("#[assert(one_of(a))] struct G { a: Option<LitStr> }");
        assert!(one.contains("at least 2"), "{one}");
        assert!(one.contains("1 was named"), "{one}");
    }

    #[test]
    fn a_rule_we_do_not_have_is_named_with_the_ones_we_do() {
        let message = rejected(
            "#[assert(exactly_two(a, b))] struct G { a: Option<LitStr>, b: Option<LitStr> }",
        );

        assert!(message.contains("one_of"), "{message}");
    }

    #[test]
    fn with_written_as_a_call_is_corrected() {
        // `with` names a type rather than keys, so `with(Rule)` is a plausible slip worth catching
        // precisely instead of failing as 'no field named Rule'.
        let message = rejected("#[assert(with(SomeRule))] struct G { a: Option<LitStr> }");

        assert!(message.contains("`with = SomeRule`"), "{message}");
    }

    #[test]
    fn an_authors_rule_is_called_against_self() {
        let out = expand("#[assert(with = NoZeroRetries)] struct G { a: Option<LitStr> }")
            ;

        assert!(out.contains("NoZeroRetries as :: proc_macro_flow_traits :: assert :: Rule"), "{out}");
        assert!(out.contains(":: check (self , out)"), "{out}");
    }
}
