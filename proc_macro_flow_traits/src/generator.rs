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

use crate::extractor::Extraction;

/* @group(#generation/composition)
 *
 * DESIGNED, NOT BUILT. How a macro says what it generates, so a failure names the THING that
 * failed instead of arriving as an opaque error in a TokenStream.
 *
 * NOTE(#generation/newtype-per-item): V[S(Item).wraps(syn) && Ty(Output) == Self], "One NEWTYPE per
 * generated item - `struct Fields(syn::ImplItem)` - and the newtype IS the Ty(Output). This
 * collapses a distinction that looked real: 'one generator, one output' and 'one generator, many
 * outputs' are the same pattern once each output is its own type, because many outputs become many
 * small generators composed by a parent. There is no mode to select.
 *
 * It also fixes a COLLISION that the alternatives share. Marking generators by their syn output
 * type (`Generator<syn::ItemConst>`) breaks the moment two items have the same syn type, and
 * `FIELDS` and `SHAPES` are both consts - the first real example hits it. Newtypes are distinct by
 * construction.
 *
 * The newtype is then the one home for four things that are currently absent or stringly: the
 * item's NAME, its SPAN (ID(typed-output/spans)), its STUB, and its FAILURE IDENTITY - a failed
 * generation is attributable because a TYPE failed, not because someone remembered to write the
 * name into a message"
 *
 * NOTE(#generation/extraction-shape): V[F(generate).R(Extraction<Output>)], "F(generate) returns
 * Ty(Extraction<Output>) - value plus reasons - and NOT Ty(Result). With a Result a parent must
 * either abort on its first failed child or invent a side channel for the rest, and aborting is
 * exactly the sibling-dropping ID(no-result) exists to prevent.
 *
 * Ty(Extraction) already means 'here is what I built AND what went wrong', F(absorb) already
 * gathers a child's reasons across whether or not it produced a value, and both are what the other
 * two stages use. So generation stops being the odd one out: three stages, one primitive, one
 * accumulation rule. It is a SIMPLIFICATION - S(Errors), the Result, and Tr(Pipeline)::run's
 * degrade-to-stub logic all collapse into F(absorb).
 *
 * CORRECTION to an earlier draft of this note, kept because the mistake is instructive: it claimed
 * F(stub) becomes INFALLIBLE, turning NOTE(#generator/stub-is-not-empty) from 'usually' into
 * 'always'. It does not. A stub is built by PARSING (`parse2(quote!{..})`), so infallibility needs
 * either M(parse_quote) - the panic NOTE(#generator/parse-quote-panics) bans - or hand-constructing
 * syn::ItemImpl field by field. F(stub) stays fallible, and 'always' keeps coming from where it
 * already came from: Tr(Pipeline)::run degrading when even the stub fails"
 *
 * NOTE(#generation/parent-feeds-children): V[Attr(generates).A(expr)], "A parent declares its
 * children AND what it feeds each one: `#[generates(fields: Vec<Fields> = input.fields.iter())]`.
 * That is Attr(from) pointed the other way, and deliberately the same bargain - the expression is
 * spliced verbatim and never inspected, so a bad one is rustc's error at the AUTHOR's span.
 *
 * ARITY IS IN THE TYPE, as everywhere else (ID(from/arity-from-type)). It has to be WRITTEN here
 * rather than read off a field, because a generator newtype is a tuple struct with no field type to
 * read it from - that is the one place this syntax is noisier than Attr(from), and the alternative
 * was a second vocabulary for arity, which is the darling failure mode.
 *
 * It also settles the flat-vs-tree question by construction: if parents feed children, the children
 * ARE a tree. A flat set has nobody to do the feeding"
 *
 * NOTE(#generation/stub-takes-context): V[F(stub).A(Subject)], "F(stub) receives context FROM THE
 * PARENT rather than being a constant, because the parent may know a condition that changes what a
 * vacant child should look like. A leaf cannot know the impl it will sit inside; its parent does"
 *
 * NOTE(#generation/no-crate-flag): V[!feature.selects(pattern)], "REJECTED, and recorded so it is
 * not proposed again. The idea was a cargo feature choosing whether one-output-per-generator is
 * enforced. Two mechanical problems, neither stylistic. Features are UNIFIED across the dependency
 * graph, so two crates wanting different policies in one build cannot both have theirs - the one
 * that did not ask gets it anyway. And features must be ADDITIVE: a flag that RESTRICTS breaks code
 * that compiled without it, which is the failure where `cargo build` works and
 * `cargo build --all-features` does not.
 *
 * Moot in the end - ID(generation/newtype-per-item) removes the choice the flag was selecting"
 *
 * TODO[ ](#generation/pipeline-module):C[Attr(pipeline).on(mod)], "Attr(pipeline) wraps a MODULE
 * holding the pipeline's components, each declaring where it comes from: `#[generator(from =
 * Processed)]`. The declaration is NECESSARY, not decorative - a proc macro sees TOKENS and cannot
 * resolve types, so it can never read `type Input = ..` and learn that it names the extractor's
 * output. That is the same constraint that forces Attr(source) to exist at all.
 *
 * A module is the smallest scope that hands the macro the WHOLE GRAPH at once, which is what lets
 * it emit the Tr(Pipeline) impl, check that every `from` names something present, and say so
 * usefully when it does not - none of which it can do seeing one item at a time"
 *
 * TODO[ ](#generation/composition-derive):C[Attr(generator)], "The derive: emit Tr(Generator)'s
 * plumbing and, for a parent, the composition - call each child with its fed input, F(absorb) the
 * result, STUB the ones that failed, assemble. The author writes only the leaf bodies, per the same
 * split ID(processor/derive-enforces) settles for processing: derive the plumbing, require the
 * logic.
 *
 * The payoff is per-child failure ISOLATION - a failed child is stubbed, its siblings survive, and
 * one reason names the child by its type"
 */

/* @group(#typed-output)
 *
 * NOTE(#generator/parse-quote-panics): `parse_quote!` is BANNED in a generator - it panics, and a
 * panic in a proc macro lands in the AUTHOR'S compile as an opaque failure with no span. Use
 * `parse2(..)?`; F(generate) and F(stub) return syn::Result and Tr(Pipeline)::run degrades to the
 * stub, then to the error alone.
 * NOTE(#typed-output/not-the-carriers): V[S(Unresolved).T(TokenStream) && S(ListBody).T(TokenStream)],
 * "Recorded so the TODOs above are not read as 'replace every TokenStream'. Unresolved and ListBody
 * hold raw tokens BECAUSE they are unparsed - ID(no-parse) and ID(openings) exist to keep them that
 * way, and typing them would defeat the deferral the whole design rests on. The rule is: type what
 * WE build, leave what the USER wrote alone until someone asks it a question"
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

    fn generate(input: Self::Input) -> Extraction<Self::Output>;

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

        fn generate(input: &'static str) -> Extraction<ItemImpl> {
            let body = syn::LitStr::new(input, proc_macro2::Span::call_site());
            Extraction::value(parse_quote!(impl Thing for T { const NAME: &'static str = #body; }))
        }

        fn stub(_: &'static str) -> syn::Result<ItemImpl> {
            Ok(parse_quote!(impl Thing for T { const NAME: &'static str = ""; }))
        }
    }

    #[test]
    fn a_generator_builds_a_typed_item() {
        let item = Tiny::generate("thing").value.expect("well formed");
        // it is an ItemImpl, so the SHAPE is inspectable rather than a string to grep
        assert_eq!(item.items.len(), 1);
    }

    #[test]
    fn the_stub_is_the_same_shape_and_the_type_says_so() {
        // THE rule, now enforced structurally. Both arms return ItemImpl with the same associated
        // items, so a use site finds NAME either way and never reports a missing item on top of
        // the real diagnostic. Previously this could only be asserted by string-matching a stream.
        let full = Tiny::generate("thing").value.expect("well formed");
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
        let item = Tiny::generate("thing").value.expect("well formed");
        assert!(item.trait_.is_some(), "the impl lost its trait");
    }
}
