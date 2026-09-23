// @review [ ]
//! The pipeline macro: an extractor, a processor and a generator, like everything else.

pub(crate) mod extractor;
pub(crate) mod generator;
pub(crate) mod processor;

use proc_macro2::TokenStream;
use proc_macro_flow_traits::extractor::Extractor;
use proc_macro_flow_traits::generator::Generator;
use proc_macro_flow_traits::processor::Processor;
use proc_macro_flow_traits::render::Diagnose;
use quote::ToTokens;
use syn::ItemMod;

use extractor::PipelineExtraction;
use generator::{PipelineArgs, PipelineExpansion, PipelineInput};

/// Run the three stages by hand.
///
/// This is the one entry point in the crate that does NOT call Tr(Pipeline)::run, and the reason is
/// recorded at NOTE(#pipeline/subject-equals-source-breaks-attributes): an attribute macro has two
/// inputs, and Tr(Pipeline) binds the generator's Subject to the extractor's Source, so there is no
/// Source carrying both. The stages themselves follow the pattern exactly; only the orchestration
/// is bespoke.
pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let module = match syn::parse2::<ItemMod>(item.clone()) {
        Ok(module) => module,
        // The author's item is NOT deleted on a parse failure - an attribute macro replaces what it
        // annotates, so emitting only the error would delete their code.
        Err(error) => return with(item, error),
    };

    let args = match syn::parse2::<PipelineArgs>(attr) {
        Ok(args) => args,
        Err(error) => return with(module.to_token_stream(), error),
    };

    let extracted = PipelineExtraction::extract_from(&module);
    let mut errors = extracted.render();

    let processed = PipelineExtraction::process(extracted);
    errors.extend(
        processed
            .reasons
            .iter()
            .map(|reason| reason.to_error(&module, reason.message())),
    );

    let Some(pipeline) = processed.value else {
        return with(module.to_token_stream(), combine(errors));
    };

    let input = PipelineInput {
        processed: pipeline,
        args: &args,
        module: &module,
    };

    let generated = PipelineExpansion::generate(&input);
    errors.extend(
        generated
            .reasons
            .iter()
            .map(|reason| reason.to_error(&module, reason.message())),
    );

    let expansion = match generated.value {
        Some(expansion) => expansion.to_token_stream(),
        None => match PipelineExpansion::stub(&module) {
            Ok(vacant) => vacant.to_token_stream(),
            Err(error) => {
                errors.push(error);
                module.to_token_stream()
            }
        },
    };

    match combine_all(errors) {
        Some(error) => with(expansion, error),
        None => expansion,
    }
}

/// Emit what was built, then the complaint.
fn with(built: TokenStream, error: syn::Error) -> TokenStream {
    let mut out = built;
    out.extend(error.to_compile_error());
    out
}

fn combine(errors: Vec<syn::Error>) -> syn::Error {
    combine_all(errors)
        .unwrap_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "the pipeline failed"))
}

fn combine_all(errors: Vec<syn::Error>) -> Option<syn::Error> {
    errors.into_iter().reduce(|mut all, next| {
        all.combine(next);
        all
    })
}
