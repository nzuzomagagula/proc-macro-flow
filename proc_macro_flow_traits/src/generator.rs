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

use proc_macro2::TokenStream;

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

/// Emit code from a processed value.
///
/// NOTE(#generator/stub-is-a-contract): V[Tr(Generator).M(stub).required], "F(stub) is a REQUIRED
/// method, not an inherent convenience a generator may or may not have written. ID(generator/
/// stub-is-not-empty) was a RULE the entry point had to remember: match on the value, generate or
/// stub, then append the errors - four lines that had to be right at every entry point, and wrong
/// in exactly one of them is a cascade of 'no associated item named ..' at every use site. As a
/// contract the rule cannot be forgotten: a type that cannot say what its vacant form looks like
/// does not compile as a Generator, and F(emit) below is the only way to spend it"
pub trait Generator: Sized {
    /// A processor's `Output`.
    type Input;

    /// The item the output is written against - what a STUB names when there is no value to
    /// generate from. Separate from `Input` precisely because the stub case has no `Input`.
    type Item;

    // TODO[ ](#typed-output/generate): see @group(#typed-output) above - this should be a typed
    // syn item, not a raw stream.
    fn generate(input: Self::Input) -> TokenStream;

    /// The same output shape, vacant.
    ///
    /// Same associated items as a successful generation, none of the content. NOT an empty stream:
    /// that leaves every use site reporting a missing item on top of the real diagnostic, which is
    /// the cascade the whole rule exists to prevent.
    fn stub(item: &Self::Item) -> TokenStream;

    /// The whole emission, with the stub-always rule built in.
    ///
    /// This is what an entry point calls. There is deliberately no way to emit errors WITHOUT a
    /// stub, and no way to emit a stub without having said what a vacant one looks like.
    fn emit(value: Option<Self::Input>, item: &Self::Item, errors: Vec<syn::Error>) -> TokenStream {
        // The stub goes out FIRST whatever happened, then one `compile_error!` per error.
        let mut out = match value {
            Some(value) => Self::generate(value),
            None => Self::stub(item),
        };
        out.extend(errors.into_iter().map(|error| error.to_compile_error()));
        out
    }
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
