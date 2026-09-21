// @review [ ]
//! The macro boundary: the one place a pipeline becomes a `TokenStream`.
//!
//! NOTE(#pipeline/owns-normalisation): V[Tr(Pipeline).M(run) == F(field_names).body], "Everything
//! between 'I have a parsed node' and 'here is what the macro returns' is IDENTICAL for every
//! macro: run the stages in order, walk the tree for reasons before processing consumes it, choose
//! generate-or-stub, lower, append one compile_error per reason. That was hand-written in
//! lib.rs::field_names and would have been copy-pasted at every entry point, with the stub rule to
//! get right each time.
//!
//! It is NORMALISATION, not pipeline logic, so it lives on the type that owns the macro boundary
//! and the three stages stay clean. A generator knows the vacant SHAPE; the pipeline decides WHEN
//! to use it"
//!
//! NOTE(#pipeline/not-a-description): V[Tr(Pipeline).!restates(S(Extraction))], "Amends
//! Answer(#extractor/self-hosting), which concluded 'nothing holds a triple, because nothing needs
//! to'. That OVERREACHED. The evidence supported only the narrower claim - nothing needs a triple
//! to DESCRIBE the extraction's shape - and that part stands: the field type carries the arity and
//! Ty(Extracted) carries the source, so a struct restating either would be redundant.
//!
//! Owning NORMALISATION is a different job with a different justification, and the original warning
//! is the guard rail rather than the refutation: if Tr(Pipeline) ever grows a field or a method
//! that restates what the extraction struct already says, that is the drift the old answer was
//! right about"

use proc_macro2::TokenStream;
use quote::ToTokens;

use crate::extractor::{Extractor, Validate};
use crate::generator::Generator;
use crate::processor::Processor;
use crate::render::Diagnose;

/// The three stages, run as one.
pub trait Pipeline<'ast> {
    /// Reads the node. Its `Output` must be walkable, which is where ID(extractor/output-bound)
    /// finally gets its bound - HERE, and still not on Tr(Extractor) itself.
    type Extractor: Extractor<'ast>;

    /// Understands the extraction.
    ///
    /// NOTE(#pipeline/no-processor-is-the-extractor): V[Ty(Processor) == Ty(Extractor).allowed],
    /// "There is no two-stage variant and no Option. A pipeline with nothing to process names its
    /// EXTRACTOR here, because an extraction type may implement both - which is already what
    /// StructExtraction and FieldExtraction do. So 'two or three stages' is expressed by naming
    /// the same type twice rather than by a branch in the type system"
    type Processor: Processor<Input = <Self::Extractor as Extractor<'ast>>::Output>;

    /// Builds the typed output.
    type Generator: Generator<
            Input = <Self::Processor as Processor>::Output,
            Subject = <Self::Extractor as Validate<'ast>>::Source,
        >;

    /// Run every stage and normalise the result.
    ///
    /// The order is not arbitrary. `render` takes the tree BORROWED and must happen before
    /// `process` consumes it, which is the whole reason the walk returns a collection rather than
    /// a stream (NOTE(#render/who-renders)).
    fn run(node: <Self::Extractor as Validate<'ast>>::Source) -> TokenStream
    where
        <Self::Extractor as Extractor<'ast>>::Output: Diagnose,
        <Self::Extractor as Validate<'ast>>::Source: Copy + ToTokens,
    {
        let extracted = Self::Extractor::extract_from(node);

        // Every reason in the tree, collected while the tree still exists.
        let mut errors = extracted.render();

        let processed = Self::Processor::process(extracted);
        errors.extend(
            processed
                .reasons
                .iter()
                .map(|reason| reason.to_error(&node, reason.message())),
        );

        // The stub goes out whatever happened - NOTE(#generator/stub-is-not-empty). There is
        // deliberately no path here that emits errors without one.
        //
        // NOTE(#pipeline/generation-degrades): V[F(run).!panics], "Generation can FAIL now rather
        // than panic (DEPRECATED(#generator/parse-quote-panics)), so this degrades in two steps:
        // a failed generate falls back to the STUB, and a failed stub emits the errors alone.
        // The second case is the only one that breaks ID(generator/stub-is-not-empty)'s promise,
        // and it is the case where keeping it is impossible - the generator could not say what its
        // vacant form looks like. Either way the author gets a diagnostic instead of a crash"
        let body = match processed.value {
            Some(value) => Self::Generator::generate(value),
            None => Self::Generator::stub(node),
        };

        let mut out = match body {
            Ok(item) => item.into_token_stream(),
            Err(failure) => match Self::Generator::stub(node) {
                Ok(vacant) => {
                    errors.push(failure);
                    vacant.into_token_stream()
                }
                Err(_) => {
                    errors.push(failure);
                    TokenStream::new()
                }
            },
        };

        out.extend(errors.into_iter().map(|error| error.to_compile_error()));
        out
    }
}

// TODO[ ](#pipeline/helpers-are-syntactic):C[Tr(Pipeline).C(HELPERS)], "Helper attributes should be
// declared ONCE, by the extractor that reads them, and queried from here - nothing should restate
// the list. The obstacle is syntactic: `#[proc_macro_derive(Name, attributes(a, b))]` takes LITERAL
// IDENTS at the macro's definition site, so a const cannot feed it. Single-source-of-truth is
// therefore reachable two ways - generate the entry point from the const, or keep the list written
// and assert agreement with it - and the choice is worth making deliberately. Until then the list
// stays hand-synced, and a missed name fails at the USER's site with no clue why, which is exactly
// the cost this item exists to remove"
// TODO[ ](#pipeline/macro-kind):C[Tr(Pipeline).A(kind)], "Only the DERIVE shape is built: take a
// node, return new items. An attribute macro must also RE-EMIT the item it was applied to, and a
// function-like macro takes tokens rather than a parsed node - both change what F(run) accepts and
// returns. Adapting is the pipeline's job by ID(pipeline/owns-normalisation), so it belongs here
// rather than in each entry point"
