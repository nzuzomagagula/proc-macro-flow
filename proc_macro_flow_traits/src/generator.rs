// @review [ ]
//! The generation stage: emitting code, and emitting it even when something went wrong.
//!
//! NOTE(#generator/stub-alongside-errors): V[F(emit).always_includes(stub)], "A generator MUST emit
//! its stub even when the extraction failed, and this is not a stylistic preference. A generator
//! that returns only compile_error!s leaves the impl missing, and every use site then reports
//! 'does not implement ..' - N follow-on errors that bury the one real diagnostic underneath them.
//! The floor is therefore a stub item BESIDE the errors, always. `emit` below exists so that is
//! mechanical rather than remembered, the same way walk_keys owns unknown-key reporting instead of
//! trusting each caller to do it"
//!
//! NOTE(#generator/one-input-shape): V[Ty(Input) == Tr(Processor).Ty(Output)], "A generator consumes
//! a processor's Output and nothing else. That is only ONE shape because processing is never
//! skipped - an extraction with no real processing derives the identity Processor, so the stage
//! always ran. Making the generator accept 'either an extraction or a processed value' was the
//! alternative, and it would have put that either-ness in every downstream signature forever"

use quote::ToTokens;

/* @group(#typed-output)
 *
 * TODO[ ](#typed-output/generate): U[Tr(Generator).F(generate).R(TokenStream) -> R(syn::Item)],
 * "generate returns a raw TokenStream, and should return a TYPED syn item - ItemImpl for the usual
 * case, Item where a generator emits more than one kind. The distinction that matters is CARRIER vs
 * PRODUCT: raw tokens are correct wherever the content is deliberately un-interpreted, which is
 * Unresolved and ListBody and must NOT change - the whole point there is that nobody has read them.
 * A generator is the opposite end: WE produce the content, we know its shape, and handing it back
 * as an untyped stream throws that away. What it costs today is (1) nothing checks that what we
 * emitted is even well-formed until rustc parses it back, and (2) it makes ID(typed-output/spans)
 * below impossible"
 *
 * TODO[ ](#typed-output/spans): U[F(emit).A(node)], "Errors are currently spanned against whatever
 * node the CALLER passes, which for generated code means pointing at the whole stream and saying
 * 'something in here is wrong'. With a typed item the generator can point at the PART that is
 * wrong - the associated type that could not be filled, the field whose value never arrived - by
 * spanning the specific ImplItem or Field rather than the item entire. Note this is the same
 * argument ID(reason/span-not-node) already makes one level down: a reason that points at one token
 * is exact, and one that points at everything is a shrug. syn 3 also has Error::new_range
 * (error.rs:267) for spanning a cursor range, which is the precise tool for 'this part of what we
 * built', and nothing here uses it yet"
 *
 * NOTE(#typed-output/not-the-carriers): V[S(Unresolved).T(TokenStream) && S(ListBody).T(TokenStream)],
 * "Recorded so the TODOs above are not read as 'replace every TokenStream'. Unresolved and ListBody
 * hold raw tokens BECAUSE they are unparsed - ID(no-parse) and ID(openings) exist to keep them that
 * way, and typing them would defeat the deferral the whole design rests on. The rule is: type what
 * WE build, leave what the USER wrote alone until someone asks it a question"
 */

/// Build a typed item from a processed value.
///
/// NOTE(#generator/stub-is-a-contract): V[Tr(Generator).M(stub).required], "F(stub) is a REQUIRED
/// method, not an inherent convenience a generator may or may not have written. ID(generator/
/// stub-is-not-empty) was a RULE the entry point had to remember: match on the value, generate or
/// stub, then append the errors - four lines that had to be right at every entry point, and wrong
/// in exactly one of them is a cascade of 'no associated item named ..' at every use site. As a
/// contract the rule cannot be forgotten: a type that cannot say what its vacant form looks like
/// does not compile as a Generator.
///
/// The rule got STRONGER when Ty(Output) became typed: the stub and the success now have to be the
/// same Rust TYPE, so 'same shape, vacant' is checked by the compiler rather than by a test reading
/// strings out of a TokenStream"
///
/// NOTE(#generator/lowering-is-not-ours): V[!Tr(Generator).M(emit)], "F(emit) used to live here and
/// has MOVED to Tr(Pipeline). A generator's job is to build a typed item and to say what its vacant
/// form looks like; turning that into a TokenStream, appending compile_errors and matching the
/// macro's expected return type is NORMALISATION, which is identical for every macro and so belongs
/// to the type that owns the macro boundary. The split: the generator knows the vacant SHAPE, the
/// pipeline decides WHEN to use it"
pub trait Generator: Sized {
    /// A processor's `Output`.
    type Input;

    /// What a STUB is written against - the node the output is generated *about*.
    ///
    /// Separate from `Input` precisely because the stub case has no `Input`. Named `Subject` and
    /// not `Item` deliberately: `syn::Item` is now a plausible `Output`, and one trait carrying
    /// both meanings of the word is how this design gets confusing.
    type Subject;

    /// The typed item this generator produces.
    ///
    /// NOTE(#typed-output/level-is-associated): V[Ty(Output).assoc], "An ASSOCIATED TYPE rather
    /// than a fixed syn::Item, and that is load-bearing for ID(generator/nested-items). Typed
    /// nesting does not compose with one type: a module holds Item, an impl block holds ImplItem,
    /// a struct holds Field. Fixing Output to syn::Item would make every generator that emits at a
    /// different level lower to tokens early, which is the thing typing it was meant to stop. An
    /// associated type lets each generator name its own level, so a parent can embed a child's
    /// ImplItem in its own ItemImpl and the tree stays typed all the way down"
    type Output: ToTokens;

    fn generate(input: Self::Input) -> Self::Output;

    /// The same output, vacant.
    ///
    /// Same associated items as a successful generation, none of the content. NOT an empty stream:
    /// that leaves every use site reporting a missing item on top of the real diagnostic, which is
    /// the cascade the whole rule exists to prevent.
    fn stub(subject: Self::Subject) -> Self::Output;
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    /// A generator whose vacant form is observably the SAME SHAPE as its full one - which is the
    /// property NOTE(#generator/stub-is-a-contract) exists to force.
    struct Tiny;

    impl Generator for Tiny {
        type Input = &'static str;
        type Item = &'static str;

        fn generate(input: &'static str) -> TokenStream {
            let body = syn::LitStr::new(input, proc_macro2::Span::call_site());
            quote!(impl Thing for T { const NAME: &'static str = #body; })
        }

        fn stub(_: &&'static str) -> TokenStream {
            quote!(impl Thing for T { const NAME: &'static str = ""; })
        }
    }

    fn errors(n: usize) -> Vec<syn::Error> {
        (0..n)
            .map(|_| syn::Error::new(proc_macro2::Span::call_site(), "bad"))
            .collect()
    }

    #[test]
    fn the_stub_survives_the_errors() {
        // THE property. Without this the missing impl cascades at every use site.
        let out = Tiny::emit(None, &"thing", errors(2)).to_string();

        assert!(out.contains("impl Thing for T"), "{out}");
        assert_eq!(out.matches("compile_error").count(), 2, "{out}");
    }

    #[test]
    fn a_clean_generation_is_just_the_value() {
        let out = Tiny::emit(Some("thing"), &"thing", vec![]).to_string();

        assert!(out.contains("impl Thing for T"));
        assert!(out.contains("thing"));
        assert!(!out.contains("compile_error"));
    }

    #[test]
    fn the_stub_comes_first() {
        // Order matters for readability of the emitted file, and for anyone reading expansion.
        let out = Tiny::emit(None, &"thing", errors(1)).to_string();

        assert!(
            out.find("impl").unwrap() < out.find("compile_error").unwrap(),
            "{out}"
        );
    }

    #[test]
    fn a_failure_and_a_success_emit_the_same_associated_items() {
        // The stub is not an empty stream: a use site must find NAME either way, or it reports a
        // missing item on top of the real diagnostic.
        let full = Tiny::emit(Some("thing"), &"thing", vec![]).to_string();
        let vacant = Tiny::emit(None, &"thing", errors(1)).to_string();

        assert!(full.contains("const NAME"), "{full}");
        assert!(vacant.contains("const NAME"), "{vacant}");
    }
}
