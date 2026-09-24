// @review [~]
// NOTE(#derive/inception-is-the-point): V[N(x).N(x).is(deliberate)], "Module inception is the pattern, not an accident"
// `clippy::module_inception` fires on N(extractor/extractor), N(processor/processor) and
// N(generator/generator), and each one is exactly what ID(derive/structure-mirrors-the-pattern)
// asked for: a derive is a pipeline, so its directory holds the three stages, and one of those
// stages shares the derive's own name. Renaming to silence the lint would cost the property the
// structure exists to carry.
//
// Allowed here rather than per module so the decision sits in one place. N(base/extractor) has had
// the same shape since long before the lint was looked at.
#![allow(clippy::module_inception)]

use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{DeriveInput, parse_macro_input};

use proc_macro_flow_traits::pipeline::Pipeline;

use crate::base::extractor::pipeline::ExtractorPipeline;

// TODO[ ](#cleanup/pipeline-home): M[N(base/pipeline) => N(pipeline)], "#[pipeline] is the product and does not belong in base"
// `base` held three unrelated things; the macro that writes macros is the one that is live.
// TODO[ ](#cleanup/retire-syntax-prototype): D[N(base/syntax)], "The pre-derive syntax stage is superseded by derive(Syntax)"
// Its still-true design moves into derive/syntax; what it promised and nothing implements becomes
// open #syntax items.
// TODO[ ](#cleanup/retire-field-names): D[N(base/extractor)] && D[MacDef(FieldNames)], "The hand-written FieldNames proof is used by nothing but itself"
// Behaviour its tests proved that no live test covers is recorded as a gap before it goes.
mod base;
mod derive;

// Answer(#extractor/entry-is-generated): A[ID(extractor/entry-is-generated) ==? F(field_names)], "This entry stays hand-written"
// this entry STAYS hand-written, and the plan that asked
// for it to be regenerated with Attr(pipeline) was asking for something rustc forbids.
//
// `can't use a procedural macro from the same crate that defines it` - ID(derive/cannot-self-host),
// already VERIFIED for the derives and true of Attr(pipeline) for exactly the same reason. It is
// defined here, so it cannot be applied here, and no arrangement of modules changes that.
//
// The point of the step was never this function though - it was to compile the entry that
// Attr(pipeline) GENERATES, which until then had only ever been asserted as tokens under
// `entry = manual`. That is done, in a crate that can host it: see
// ID(demo/why-a-third-crate). It found a bug that had made the whole path uncompilable -
// ID(pipeline-macro/wiring-is-private).
#[proc_macro_derive(FieldNames)]
pub fn field_names(input: TokenStream) -> TokenStream {
    let derive_input = parse_macro_input!(input as DeriveInput);

    // The only lowering in the whole crate that is not rustc's own signature.
    ExtractorPipeline::run(&derive_input).to_token_stream().into()
}

// ===========================================================================
// THE DERIVES - generating extraction logic instead of writing it out
// ===========================================================================
//
// See @group in derive/mod.rs for why these are three and not one, and for why this crate can
// never use them on its own types.

/// Generate `extract_from` from `#[source(Ty)]` and each field's `#[from]` / `#[with]`.
#[proc_macro_derive(Extractor, attributes(source, args, from, with, value))]
pub fn extractor(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);
    derive::ExtractorWiring::run(&parsed).to_token_stream().into()
}

/// Generate the trivial pass-through `Validate`. Omit it when there is a real narrowing to do.
///
/// Parse and run, and nothing else - everything between 'I have a node' and 'here is a token
/// stream' is Tr(Pipeline)'s (ID(pipeline/owns-normalisation)).
#[proc_macro_derive(Validate, attributes(source, args))]
pub fn validate(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);
    derive::ValidateWiring::run(&parsed).to_token_stream().into()
}

/// Wire a pipeline, and generate the macro entry point for it.
///
/// `#[pipeline(derive = Name)]` over a module of `#[extractor]` / `#[processor]` / `#[generator]`
/// components emits the module back, its `Pipeline` impl, and the `#[proc_macro_*]` function.
/// `entry = manual` omits the last of those.
///
/// MUST SIT AT THE CRATE ROOT. VERIFIED: `functions tagged with #[proc_macro_derive] must currently
/// reside in the root of the crate`, and the entry point is emitted as a SIBLING of this module -
/// see ID(pipeline/entry-is-a-sibling). The macro cannot check where it was invoked, so a
/// misplaced one fails with rustc's own message, which at least says exactly what is wrong.
#[proc_macro_attribute]
pub fn pipeline(attr: TokenStream, item: TokenStream) -> TokenStream {
    crate::base::pipeline::expand(attr.into(), item.into()).into()
}

/// Declare a grammar node: a struct of fields becomes something readable from a `syn::Meta`.
///
/// `#[shape(AttributeKind::MetaList)]` narrows which openings a field accepts and lowers to a
/// trait BOUND (ID(shape/bound-at-last)). `#[alias]` adds the standard case spellings;
/// `#[alias("x")]` adds exactly what it names.
#[proc_macro_derive(Syntax, attributes(shape, alias, assert))]
pub fn syntax(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);
    derive::SyntaxWiring::run(&parsed).to_token_stream().into()
}

/// Write a type's `Diagnose` walk, and the `Assert` its supertrait requires.
///
/// Every field is descended into unless marked `#[skip]`, by BOTH: the walk reaches the children,
/// and the `Assert` asks each field for its rules - so a grammar held as a plain value has its
/// `#[assert(..)]` checked. That default is deliberate: a field wrongly walked is a compile error,
/// while one wrongly skipped compiles and silently loses every diagnostic and rule beneath it - see
/// ID(diagnose-derive/walk-all-and-skip) and ID(diagnose-derive/asks-what-it-walks).
#[proc_macro_derive(Diagnose, attributes(skip))]
pub fn diagnose(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);
    derive::DiagnoseWiring::run(&parsed).to_token_stream().into()
}

/// Declare a generator: what it consumes, and which children it composes.
///
/// `#[builds(from = Ty, subject = Ty)]` wires it; `#[generates(name: Ty = expr)]` declares each
/// child and what the parent feeds it. The author supplies `assemble` and `assemble_stub` — the
/// derive cannot know what SHAPE the parent's item is. See
/// ID(generator-derive/plumbing-not-logic).
// TODO[x](#generator-derive/wiring-renamed): R[Attr(generator).on(derive) -> Attr(builds)], "One head meant two things, so it was renamed to #[builds]"
// One head meaning two things, and F(stripped) with no way to tell them apart - so a generator
// inside a pipeline module could not use its own derive
// NOTE(#generator-derive/wiring-has-its-own-name): V[Attr(builds) != E(Role).V(Generator)], "#[builds] no longer collides with the generator role"
// Was `#[generator(from = Ty, subject = Ty)]`, which collided head-for-head with Attr(pipeline)'s
// GENERATOR ROLE. A generator inside a pipeline module therefore could not use this derive at all:
// F(stripped) removes an attribute by its head, and with one head meaning two things it took the
// derive's wiring away with the role it was asked to strip.
//
// Nothing could have disambiguated them - a head is all F(stripped) has to go on - so one of the
// two had to move, and this is the cheaper: the role names appear in every pipeline module, while
// this is written only by an author of a generator. Attr(generates) never collided and is
// untouched.
#[proc_macro_derive(Generator, attributes(builds, generates))]
pub fn generator(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);
    derive::GeneratorWiring::run(&parsed).to_token_stream().into()
}

/// Generate the identity `Processor`. Omit it when the stage does real work.
#[proc_macro_derive(Processor, attributes(source, args))]
pub fn processor(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);
    derive::ProcessorWiring::run(&parsed).to_token_stream().into()
}

// TODO[x](#derive/entries-are-parse-and-run): D[F(expand)], "The shared entry could report only one error; it is gone"
// The shared entry every derive went through could report exactly ONE error, because `?` returns on
// the first - so a grammar with three mistakes showed one and cost the author two extra recompiles.
// ID(no-result) is why a pipeline cannot do that
// NOTE(#derive/every-entry-is-parse-and-run): V[N(lib) != F(expand)], "Every derive entry is parse then run"
// There was a shared F(expand) here that every derive went through: parse the input, call a
// function returning `Result<Vec<Item>>`, lower it or lower the error. It is GONE, and its absence
// is the measure of what the conversion bought.
//
// Each derive is now a Tr(Pipeline), so everything that function did - and everything it could not
// do, like walking an extraction tree for reasons before processing consumes it, or emitting a stub
// beside the errors - lives in F(run), once, for all of them. What is left at each entry point is
// the one thing rustc's signature forces: `parse_macro_input!`, run, `.into()`.
//
// It also removed a REAL limitation rather than only duplication. `Result<Vec<Item>>` could report
// exactly one error, because `?` returns on the first - so a grammar with three mistakes showed one
// and made the author recompile twice to find the others. ID(no-result) is why a pipeline cannot do
// that, and the conversion is where that guarantee reached the derives.

//TODO[ ](#future-thought/partial-derives): C[Attr(Custom), "Create attributes that point to or annotate custom implementation of things so that the derives are not all or nothing, you can choose what to include and exclude from the generated code"]
//TODO[ ](#future-thought/sub-pipelines): C[Attr(Map), "Map items in the extractor to be flagged as requiring their own processor and maybe generator source? the point is that because everything is nested, users may want a parallel pattern where once nested concept moves throughout the pipeline in different forms so we can maybe actually use sub pipelines? oay so we need to create the notion of a pipeline and be able to nest them"]
