// @review [~]
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

use proc_macro_flow_traits::{
    extractor::Extractor, generator::Generator, processor::Processor, render::Diagnose,
};

use crate::base::extractor::StructExtraction;
use crate::base::extractor::processor::ProcessedStruct;

mod base;
mod derive;

#[proc_macro_derive(HelloMacro)]
pub fn hello_macro_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = ast.ident;

    let expanded = quote! {
        impl #name {
            pub fn hello_macro() {
                println!("Hello, Macro! My name is {}!", stringify!(#name));
            }
        }
    };

    expanded.into()
}

#[proc_macro_derive(FieldNames)]
pub fn field_names(input: TokenStream) -> TokenStream {
    let derive_input = parse_macro_input!(input as DeriveInput);

    // RENAMED(#extractor/macro):R[F(extractor) -> F(field_names)], "FORCED, not chosen: two
    // Attr(proc_macro_derive(Extractor)) in one crate is `error[E0428]: the name Extractor is
    // defined multiple times` (VERIFIED), and the derives needed the name. CORRECTION to what this
    // note said first - it claimed this function never generated extraction logic and so had no
    // claim on the name. That was wrong. Its expansion was 'a stub until ExtractorPipeline::expand
    // exists', so it was the PLACEHOLDER for exactly that feature. The question it dodged - one
    // feature or two - is now settled as ONE by ID(extractor/self-hosting): the derive is that
    // feature, reached declaratively. So the name moved to the thing that earned it, and what this
    // emits - `const FIELDS` - is what it has always emitted, which the name now says"
    // TODO[x](#extractor/macro):U[F(field_names)], "The pipeline runs end to end - extract,
    // render, process, generate. ID(syntax/render)'s walk is in: every reason in the tree is
    // emitted, not just the root's. Its 'sorted by span' clause was dropped rather than done -
    // see NOTE(#render/traversal-is-source-order) for why a sort is impossible on stable AND
    // unnecessary given a depth-first walk over a source-ordered tree"
    let extracted = StructExtraction::extract_from(&derive_input);

    // The whole tree, before processing consumes it. Children's reasons were recorded faithfully
    // and never read until this walk existed - which made #no-result's guarantee half a promise.
    let mut errors = extracted.render();

    let processed = StructExtraction::process(extracted);
    errors.extend(
        processed
            .reasons
            .iter()
            .map(|reason| reason.to_error(&derive_input, reason.message())),
    );

    // One call. The stub-always rule is the trait's, not this function's, so there is no longer a
    // match here to get wrong - see NOTE(#generator/stub-is-a-contract).
    ProcessedStruct::emit(processed.value, &derive_input, errors).into()
}

// ===========================================================================
// THE DERIVES - generating extraction logic instead of writing it out
// ===========================================================================
//
// See @group in derive/mod.rs for why these are three and not one, and for why this crate can
// never use them on its own types.

/// Generate `extract_from` from `#[source(Ty)]` and each field's `#[from]` / `#[with]`.
#[proc_macro_derive(Extractor, attributes(source, from, with))]
pub fn extractor(input: TokenStream) -> TokenStream {
    expand(input, derive::derive_extractor)
}

/// Generate the trivial pass-through `Validate`. Omit it when there is a real narrowing to do.
#[proc_macro_derive(Validate, attributes(source))]
pub fn validate(input: TokenStream) -> TokenStream {
    expand(input, derive::derive_validate)
}

/// Generate the identity `Processor`. Omit it when the stage does real work.
#[proc_macro_derive(Processor, attributes(source))]
pub fn processor(input: TokenStream) -> TokenStream {
    expand(input, derive::derive_processor)
}

/// Shared entry: parse, run, and turn any error into a `compile_error!` at the author's span.
///
/// A derive that returns nothing on failure leaves the impl missing and every use site reporting
/// "does not implement", which is the cascade NOTE(#generator/stub-alongside-errors) exists to
/// prevent. Here there is no meaningful stub - the impl we failed to write IS the product - so the
/// error is all that goes out, and it is spanned where the author can act on it.
fn expand(
    input: TokenStream,
    f: fn(DeriveInput) -> syn::Result<proc_macro2::TokenStream>,
) -> TokenStream {
    let parsed = parse_macro_input!(input as DeriveInput);

    match f(parsed) {
        Ok(tokens) => tokens.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

//TODO[ ](#future-thought): C[Attr(Custom), "Create attributes that point to or annotate custom implementation of things so that the derives are not all or nothing, you can choose what to include and exclude from the generated code"]
//TODO[ ](#future-thought): C[Attr(Map), "Map items in the extractor to be flagged as requiring their own processor and maybe generator source? the point is that because everything is nested, users may want a parallel pattern where once nested concept moves throughout the pipeline in different forms so we can maybe actually use sub pipelines? oay so we need to create the notion of a pipeline and be able to nest them"]
