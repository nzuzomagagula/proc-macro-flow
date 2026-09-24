// @review [ ]
//! The macro boundary: the one place a pipeline becomes a `TokenStream`.
//!
// NOTE(#pipeline/owns-normalisation): V[Tr(Pipeline).F(run) ==? F(field_names).B(body)], "Everything between parsed node and macro output is run's job"
// Everything between 'I have a parsed node' and 'here is what the macro returns' is IDENTICAL for
// every macro: run the stages in order, walk the tree for reasons before processing consumes it,
// choose generate-or-stub, lower, append one compile_error per reason. That was hand-written in
// lib.rs::field_names and would have been copy-pasted at every entry point, with the stub rule to
// get right each time.
//
// It is NORMALISATION, not pipeline logic, so it lives on the type that owns the macro boundary
// and the three stages stay clean. A generator knows the vacant SHAPE; the pipeline decides WHEN
// to use it
//!
// NOTE(#pipeline/not-a-description): V[Tr(Pipeline) != restates(S(Extraction))], "A pipeline owns normalisation; it does not describe the grammar"
// Amends ID(extractor/self-hosting): a grammar declares itself DECLARATIVELY - Attr(source),
// Attr(from), Attr(with) read off the extraction type - so nothing holds an
// extractor/processor/generator triple to DESCRIBE a pipeline. S(ExtractorPipeline) holds one to
// own NORMALISATION, which is a different job.

use proc_macro2::TokenStream;
use quote::ToTokens;

use crate::attributed::Annotated;
use crate::extractor::{Extraction, Extractor, Validate};
use crate::generator::Generator;
use crate::processor::Processor;
use crate::render::Diagnose;

/// What a pipeline produced: the item, and everything that went wrong building it.
///
/// NOTE(#pipeline/expansion-is-typed): V[S(Expansion).P(T) && S(Expansion) != R(Vec<Item>)], "Expansion names exactly what the macro emits, not some items"
/// Ty(T) is the GENERATOR'S OWN Ty(Output), so the return type names exactly what this macro emits
/// - not `Vec<syn::Item>`, which says 'some items' when we know precisely which. A generator whose
/// output is `struct XExpansion(ItemImpl, ItemImpl)` promises TWO IMPLS and the compiler checks it;
/// a Vec promises nothing and checks nothing.
///
/// A bare tuple cannot do this - VERIFIED that quote implements Tr(ToTokens) for no tuple - but a
/// TUPLE STRUCT can, which is why ID(generation/newtype-per-item) allows N fields rather than one.
///
/// Its Tr(ToTokens) emits the item and then one compile_error! per error, which is what makes an
/// entry function a single line and keeps Ty(TokenStream) at rustc's boundary and nowhere else
pub struct Expansion<T> {
    item: Option<T>,
    errors: Vec<syn::Error>,
}

impl<T> Expansion<T> {
    pub fn new(item: Option<T>, errors: Vec<syn::Error>) -> Self {
        Self { item, errors }
    }

    /// What was built, if anything survived.
    pub fn item(&self) -> Option<&T> {
        self.item.as_ref()
    }

    pub fn errors(&self) -> &[syn::Error] {
        &self.errors
    }
}

impl<T: ToTokens> ToTokens for Expansion<T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        // The item FIRST, whatever happened - ID(generator/stub-is-not-empty). Without it the
        // missing impl cascades into 'does not implement' at every use site.
        if let Some(item) = &self.item {
            item.to_tokens(tokens);
        }
        for error in &self.errors {
            error.to_compile_error().to_tokens(tokens);
        }
    }
}

/// An attribute macro's output: the item it was applied to, AND what the pipeline built.
///
/// NOTE(#pipeline/attribute-re-emits): V[S(Reemission).P(item)], "An attribute macro replaces its item, so it must re-emit it"
/// An attribute macro REPLACES the item it annotates, so anything it does not emit is deleted. A
/// derive is the opposite - it adds beside an item rustc keeps. That difference is why
/// F(run_attribute) exists rather than F(run) growing a flag: one shape that sometimes re-emits is
/// exactly the either-ness ID(generator/one-input-shape) argues against putting into a signature.
///
/// Typed rather than a TokenStream pair, so the item and the expansion both stay inspectable up to
/// the moment the entry function lowers them
pub struct Reemission<S, T> {
    item: S,
    expansion: Expansion<T>,
}

impl<S: ToTokens, T: ToTokens> ToTokens for Reemission<S, T> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        // The author's item FIRST - deleting it would be a far worse failure than any diagnostic.
        self.item.to_tokens(tokens);
        self.expansion.to_tokens(tokens);
    }
}

/// The three stages, run as one.
pub trait Pipeline<'ast> {
    /// Reads the node. Its `Output` must be walkable, which is where ID(extractor/output-bound)
    /// finally gets its bound - HERE, and still not on Tr(Extractor) itself.
    type Extractor: Extractor<'ast>;

    /// Understands the extraction.
    ///
    /// NOTE(#pipeline/no-processor-is-the-extractor): V[Ty(Processor) ==? Ty(Extractor)], "No processing means naming the extractor as the processor"
    ///
    /// There is no two-stage variant and no Option. A pipeline with nothing to process names its
    /// EXTRACTOR here, because an extraction type may implement both - which is already what
    /// StructExtraction and FieldExtraction do. So 'two or three stages' is expressed by naming
    /// the same type twice rather than by a branch in the type system
    type Processor: Processor<'ast, Input = <Self::Extractor as Extractor<'ast>>::Output>;

    /// Builds the typed output.
    type Generator: Generator<
            'ast,
            Input = <Self::Processor as Processor<'ast>>::Output,
            Subject = <Self::Extractor as Validate<'ast>>::Source,
        >;

    /// Run every stage and normalise the result.
    ///
    /// The order is not arbitrary. `render` takes the tree BORROWED and must happen before
    /// `process` consumes it, which is the whole reason the walk returns a collection rather than
    /// a stream (ID(render/who-renders)).
    fn run(
        node: <Self::Extractor as Validate<'ast>>::Source,
    ) -> Expansion<<Self::Generator as Generator<'ast>>::Output>
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

        // The stub goes out whatever happened - ID(generator/stub-is-not-empty). There is
        // deliberately no path here that emits errors without one.
        //
        // NOTE(#pipeline/generation-degrades): V[F(run) != panics], "Generation accumulates like the other stages and never panics"
        // Generation accumulates like the other two stages now, so the two-level degrade match
        // this used to carry collapses into F(absorb): a generator that stubbed one child and
        // succeeded at three others IS an Extraction, and says so.
        let generated = match processed.value {
            Some(value) => Self::Generator::generate(value),
            None => Extraction::default(),
        };
        errors.extend(
            generated
                .reasons
                .iter()
                .map(|reason| reason.to_error(&node, reason.message())),
        );

        // The stub is the floor and still fallible - see the correction in
        // @group(#generation/composition). If even it fails, the errors go out alone, which is the
        // one case ID(generator/stub-is-not-empty) cannot cover.
        // The stub is the floor and is still fallible - see the correction in
        // @group(#generation/composition). If even it fails, the errors go out alone.
        let item = match generated.value {
            Some(item) => Some(item),
            None => match Self::Generator::stub(node) {
                Ok(vacant) => Some(vacant),
                Err(failure) => {
                    errors.push(failure);
                    None
                }
            },
        };

        Expansion::new(item, errors)
    }

    /// The same run, for an ATTRIBUTE macro: the annotated item comes back beside the output.
    ///
    /// NOTE(#pipeline/attribute-is-bounded-not-declared): V[F(run_attribute).has(W(Annotated))], "run_attribute exists only for a Source with an item to hand back"
    /// The `Source: Annotated` bound is what makes this method EXIST only for a macro that has an
    /// item to hand back. A derive's `&DeriveInput` has no Tr(Annotated) impl, so calling this on
    /// one is a compile error rather than a macro that quietly re-emits its own input; and an
    /// attribute pipeline whose extractor omitted `args` still has a bare node for a Source, so it
    /// fails the same way. See ID(attributed/annotated-decides-the-kind).
    ///
    /// It re-emits `node.item()` and NOT `node`: the arguments were consumed reading them, and
    /// echoing them back would paste `level = \"debug\"` into the author's crate as if it were code
    fn run_attribute(
        node: <Self::Extractor as Validate<'ast>>::Source,
    ) -> Reemission<
        &'ast <<Self::Extractor as Validate<'ast>>::Source as Annotated<'ast>>::Item,
        <Self::Generator as Generator<'ast>>::Output,
    >
    where
        <Self::Extractor as Extractor<'ast>>::Output: Diagnose,
        <Self::Extractor as Validate<'ast>>::Source: Copy + ToTokens + Annotated<'ast>,
    {
        Reemission {
            item: node.item(),
            expansion: Self::run(node),
        }
    }
}

// Answer(#pipeline/helpers-are-syntactic): A[ID(pipeline/helpers-are-syntactic) ==? F(generate_entry)], "Generate the entry point, so helper spellings exist once"
// GENERATE the entry point, which is the first of the two
// routes that item weighed and the one that removes the hand-sync rather than policing it.
//
// The obstacle was real and has not gone away: `#[proc_macro_derive(Name, attributes(a, b))]` takes
// LITERAL IDENTS at the definition site, so no const can feed it. What changed is who writes the
// definition site. Attr(pipeline) reads the vocabulary named by `helpers = ..` and splices its
// spellings into the Attr(proc_macro_derive) it emits, so the list exists once - in the vocabulary
// the extractor already reads - and the syntactic requirement is met by construction.
//
// ID(pipeline-macro/helpers-are-idents) records what the splice costs: spellings are stored as
// Ty(LitStr) and `attributes(..)` wants idents, so one spelling in a hundred - `not-an-ident` -
// cannot be registered and is reported instead of panicked on.
// NOTE(#pipeline/entry-is-a-sibling): V[Attr(proc_macro_derive).is(root)] && V[Attr(proc_macro_attribute).has(siblings)], "The derive entry must be at the crate root; siblings allow it"
//
// Two facts, both VERIFIED by probe before the design leaned on them, because together they decide
// the whole shape of Attr(pipeline).
//
// (1) `functions tagged with #[proc_macro_derive] must currently reside in the root of the crate` -
// rustc's words. So Attr(pipeline) on a module can NOT emit the entry point inside it.
//
// (2) An attribute macro returns a stream REPLACING the annotated item, and that stream may hold
// several items - so it can emit `mod x { .. }` PLUS a sibling `#[proc_macro_derive] pub fn`, and
// when the module sits at the crate root so does the sibling. Probed end to end across two crates.
//
// The requirement this leaves is one the macro CANNOT check, because it does not know where it was
// invoked: Attr(pipeline) must sit at the crate root. Violating it is rustc's error from (1), which
// at least says exactly what is wrong.
// Answer(#pipeline/macro-kind): A[ID(pipeline/macro-kind) ==? F(run) && F(run_attribute)], "All three macro shapes, adapted here in the pipeline"
// all three shapes, and the adapting stayed here as
// ID(pipeline/owns-normalisation) required.
//
// DERIVE is F(run): parse a Ty(DeriveInput), return new items beside it. ATTRIBUTE is
// F(run_attribute), which wraps the same run in S(Reemission) so the annotated item goes back out -
// see ID(pipeline/attribute-re-emits) for why that is a second method rather than a flag on the
// first. FUNCTION-LIKE is F(run) again with `Source = &TokenStream`, which is expressible because
// Ty(TokenStream) is Tr(Visitable) (VERIFIED, visitable.rs:41).
//
// What the three DO NOT share is the entry signature rustc fixes for each, and that is precisely
// what Attr(pipeline) generates from E(MacroKind). So the branch lives in the macro that writes the
// entry point, once, and never in a pipeline.
//
// The honest gap, recorded rather than papered over: a function-like macro has no meaningful
// Tr(Validate) - there is no node to narrow, so `Valid = Source` and the impl accepts everything.
// No check was invented to make the shape look uniform.
