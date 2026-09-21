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
 * TODO[x](#typed-output/generate): U[Tr(Generator).F(generate).R(TokenStream) -> R(Ty(Output))],
 * "DONE. The distinction it drew is the one that shipped: CARRIER vs PRODUCT. Raw tokens stay
 * correct wherever the content is deliberately un-interpreted - Unresolved and ListBody, which did
 * NOT change, because the whole point there is that nobody has read them. A generator is the
 * opposite end: WE produce the content and know its shape, so it now returns a typed item.
 *
 * Both costs it named are paid. (1) `parse_quote!` validates what we built AT CONSTRUCTION, so a
 * malformed item is a panic naming the generator rather than a rustc error in the author's crate -
 * see NOTE(#generator/parse-quote-panics). (2) ID(typed-output/spans) is no longer impossible; it
 * is merely not done.
 *
 * ONE CORRECTION to what it asked for. It wanted `R(syn::Item)` - 'ItemImpl for the usual case,
 * Item where a generator emits more than one kind'. That would have been wrong: a FIXED return type
 * cannot nest, because the levels differ - a module holds Item, an impl holds ImplItem, a struct
 * holds Field - so any generator emitting below item level would have had to lower to tokens early,
 * which is the exact thing typing it was meant to stop. Ty(Output) is an ASSOCIATED type instead.
 * See NOTE(#typed-output/level-is-associated) and NOTE(#generator/nested-items)"
 *
 * TODO[ ](#typed-output/spans): U[Tr(Generator).Ty(Output).spans], "RESTATED - its old target,
 * F(emit), no longer exists. Lowering moved to Tr(Pipeline)::run
 * (NOTE(#generator/lowering-is-not-ours)), and with it the `node` argument this item was written
 * against, so the work now belongs to the GENERATOR and its Ty(Output) rather than to the emission
 * step.
 *
 * The substance is unchanged and is now UNBLOCKED by ID(typed-output/generate). Errors about
 * generated code are still spanned against whatever node Tr(Pipeline)::run was handed - the whole
 * subject - which says 'something in here is wrong'. Holding a typed Ty(Output) means a generator
 * can point at the PART: the ImplItem whose associated type could not be filled, the Field whose
 * value never arrived. Same argument ID(reason/span-not-node) makes one level down - a reason that
 * points at one token is exact, one that points at everything is a shrug.
 *
 * syn 3's Error::new_range (error.rs:267) spans a cursor range and is the precise tool for 'this
 * part of what we built'. Nothing uses it yet"
 *
 * NOTE(#generator/nested-items): V[Ty(Output).composes], "Generators NEST like extractors and
 * processors: a parent's Ty(Output) holds its children's, typed the whole way down, so a tree of
 * generated code is assembled from checked pieces rather than concatenated as text. That is what
 * Ty(Output) being an associated type buys - see NOTE(#typed-output/level-is-associated) - because
 * the levels genuinely differ: a module holds Item, an impl holds ImplItem, a struct holds Field.
 * The payoff is that a malformed piece fails where it was BUILT, naming the generator that built
 * it, instead of arriving in the author's crate as a parse error in code they never wrote"
 *
 * DEPRECATED(#generator/parse-quote-panics):R[M(parse_quote) -> F(parse2)], "SUPERSEDED, and the
 * escape hatch it named is now the rule. It argued parse_quote!'s panic was the RIGHT signal here,
 * because these tokens are ones WE built so a failure is a framework bug rather than bad input.
 * The premise is still true; the conclusion was wrong for one reason it did not weigh: a panic in
 * a proc macro happens during the AUTHOR'S compile and reports as an opaque macro failure with no
 * span, so the person who sees it is the one person who cannot act on it.
 *
 * F(generate) and F(stub) return syn::Result now, and Tr(Pipeline)::run degrades: generate fails ->
 * try the stub, stub fails -> emit the error alone. The framework bug still reaches someone, as a
 * diagnostic they can report rather than a crash they cannot read"
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

    fn generate(input: Self::Input) -> syn::Result<Self::Output>;

    /// The same output, vacant.
    ///
    /// Same associated items as a successful generation, none of the content. NOT an empty stream:
    /// that leaves every use site reporting a missing item on top of the real diagnostic, which is
    /// the cascade the whole rule exists to prevent.
    fn stub(subject: Self::Subject) -> syn::Result<Self::Output>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{parse_quote, ItemImpl};

    /// A generator whose vacant form is the SAME TYPE as its full one - which is now checked by
    /// the compiler rather than by reading strings, per NOTE(#generator/stub-is-a-contract).
    struct Tiny;

    impl Generator for Tiny {
        type Input = &'static str;
        type Subject = &'static str;
        type Output = ItemImpl;

        fn generate(input: &'static str) -> syn::Result<ItemImpl> {
            let body = syn::LitStr::new(input, proc_macro2::Span::call_site());
            Ok(parse_quote!(impl Thing for T { const NAME: &'static str = #body; }))
        }

        fn stub(_: &'static str) -> syn::Result<ItemImpl> {
            Ok(parse_quote!(impl Thing for T { const NAME: &'static str = ""; }))
        }
    }

    #[test]
    fn a_generator_builds_a_typed_item() {
        let item = Tiny::generate("thing").expect("well formed");
        // it is an ItemImpl, so the SHAPE is inspectable rather than a string to grep
        assert_eq!(item.items.len(), 1);
    }

    #[test]
    fn the_stub_is_the_same_shape_and_the_type_says_so() {
        // THE rule, now enforced structurally. Both arms return ItemImpl with the same associated
        // items, so a use site finds NAME either way and never reports a missing item on top of
        // the real diagnostic. Previously this could only be asserted by string-matching a stream.
        let full = Tiny::generate("thing").expect("well formed");
        let vacant = Tiny::stub("thing").expect("well formed");

        assert_eq!(full.items.len(), vacant.items.len());
        // syn types carry no PartialEq without `extra-traits`, so compare the rendered type
        assert_eq!(
            full.self_ty.to_token_stream().to_string(),
            vacant.self_ty.to_token_stream().to_string()
        );
    }

    #[test]
    fn nothing_malformed_can_leave_a_generator() {
        // parse_quote! validated the tokens as an ItemImpl at construction. A mistake is a panic
        // HERE, not a mystery error in the author's crate - NOTE(#generator/parse-quote-panics).
        let item = Tiny::generate("thing").expect("well formed");
        assert!(item.trait_.is_some(), "the impl lost its trait");
    }
}
