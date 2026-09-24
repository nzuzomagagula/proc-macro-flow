// @review [~]
use proc_macro::TokenStream;
use quote::ToTokens;
use syn::{DeriveInput, parse_macro_input};

use proc_macro_flow_traits::pipeline::Pipeline;

use crate::base::extractor::pipeline::ExtractorPipeline;

mod base;
mod derive;

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
#[proc_macro_derive(Extractor, attributes(source, args, from, with))]
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
/// see NOTE(#pipeline/entry-is-a-sibling). The macro cannot check where it was invoked, so a
/// misplaced one fails with rustc's own message, which at least says exactly what is wrong.
#[proc_macro_attribute]
pub fn pipeline(attr: TokenStream, item: TokenStream) -> TokenStream {
    crate::base::pipeline::expand(attr.into(), item.into()).into()
}

/// Declare a grammar node: a struct of fields becomes something readable from a `syn::Meta`.
///
/// `#[shape(AttributeKind::MetaList)]` narrows which openings a field accepts and lowers to a
/// trait BOUND (NOTE(#shape/bound-at-last)). `#[alias]` adds the standard case spellings;
/// `#[alias("x")]` adds exactly what it names.
#[proc_macro_derive(Syntax, attributes(shape, alias, assert))]
pub fn syntax(input: TokenStream) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);
    derive::SyntaxWiring::run(&parsed).to_token_stream().into()
}

/// Declare a generator: what it consumes, and which children it composes.
///
/// `#[generator(from = Ty, subject = Ty)]` wires it; `#[generates(name: Ty = expr)]` declares each
/// child and what the parent feeds it. The author supplies `assemble` and `assemble_stub` — the
/// derive cannot know what SHAPE the parent's item is. See
/// NOTE(#generator-derive/plumbing-not-logic).
#[proc_macro_derive(Generator, attributes(generator, generates))]
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

// TODO[x](#derive/entries-are-parse-and-run): D[F(expand)], "The shared entry every derive went
// through could report exactly ONE error, because `?` returns on the first - so a grammar with
// three mistakes showed one and cost the author two extra recompiles. ID(no-result) is why a
// pipeline cannot do that"
// NOTE(#derive/every-entry-is-parse-and-run): V[N(lib).!F(expand)], "There was a shared F(expand)
// here that every derive went through: parse the input, call a function returning
// `Result<Vec<Item>>`, lower it or lower the error. It is GONE, and its absence is the measure of
// what the conversion bought.
//
// Each derive is now a Tr(Pipeline), so everything that function did - and everything it could not
// do, like walking an extraction tree for reasons before processing consumes it, or emitting a stub
// beside the errors - lives in F(run), once, for all of them. What is left at each entry point is
// the one thing rustc's signature forces: `parse_macro_input!`, run, `.into()`.
//
// It also removed a REAL limitation rather than only duplication. `Result<Vec<Item>>` could report
// exactly one error, because `?` returns on the first - so a grammar with three mistakes showed one
// and made the author recompile twice to find the others. ID(no-result) is why a pipeline cannot do
// that, and the conversion is where that guarantee reached the derives."

//TODO[ ](#future-thought/partial-derives): C[Attr(Custom), "Create attributes that point to or annotate custom implementation of things so that the derives are not all or nothing, you can choose what to include and exclude from the generated code"]
//TODO[ ](#future-thought/sub-pipelines): C[Attr(Map), "Map items in the extractor to be flagged as requiring their own processor and maybe generator source? the point is that because everything is nested, users may want a parallel pattern where once nested concept moves throughout the pipeline in different forms so we can maybe actually use sub pipelines? oay so we need to create the notion of a pipeline and be able to nest them"]
