// @review [ ]
// TODO(#extractor/pipeline):C[S(ExtractorPipeline)], "Bare struct holding its own extractor/processor/generator triple, mirroring the StructExtraction pipeline this file already builds - the extractor stage becomes self-hosting"
// TODO(#extractor/expansion):C[F(expand)], "expand(&ExtractorPipeline) -> TokenStream first, concretely; only then wire the outer expansion (ExtractionState<StructExtraction>::visit_derive_input over DeriveInput/ItemStruct). Two separate passes - don't conflate the inner macro-of-a-macro with the outer traversal already in processor.rs"
// TODO[~](#extractor/macro-wiring):U[F(extractor)], "Wire ExtractorPipeline::expand into lib.rs::extractor once it exists. Split from #extractor/macro so the two comments stop sharing one identity - nuts keys by identity, so a snapshot was only ever seeing one of them"
use syn::{DataStruct, DeriveInput};

pub(crate) use proc_macro_flow_traits::extractor::Extraction;
use proc_macro_flow_traits::{
    extractor::{Reason, ReasonKind},
    source::Sourced,
};
use crate::{
    base::extractor::extractor::field::FieldExtraction,
    traits::{Validate, extractor::Extractor},
};
pub mod attribute;
pub mod field;

//Fix[x](#extractor/recursive-source):D[Impl(Visit<'ast> for ExtractionState<StructExtraction<'ast>>)], "RESOLVED by deletion, not by rewiring. The objection was that a macro should traverse from its OWN source type and find its children from there, never from a child's genesis syn type - and extract_from now does exactly that: it takes the DeriveInput, validates it to a DataStruct, and maps its fields. The Visit impl walked from Fields, could not name a source, and only ever reached the right node by falling through syn's default traversal. Two further reasons not to keep it: Extraction lives in proc_macro_flow_traits now, so impl Visit for it is an orphan-rule violation, and the visitor could not satisfy Sourced. The OUTER-vs-Meta/Expr distinction the note drew still holds and is ID(extractor/expansion)'s business"

pub(crate) struct StructExtraction<'ast> {
    // Held so the node can say where it came from - see NOTE(#source-not-span) in
    // proc_macro_flow_traits::source for why this is the DeriveInput and not a Span.
    pub(crate) derive_input: &'ast DeriveInput,
    pub(crate) fields: Vec<Extraction<FieldExtraction<'ast>>>,
}

impl<'ast> Sourced<'ast> for StructExtraction<'ast> {
    type Source = DeriveInput;

    fn source(&self) -> &'ast DeriveInput {
        self.derive_input
    }
}

impl<'ast> Extractor<'ast, &'ast DeriveInput> for StructExtraction<'ast> {
    fn extract_from(node: &'ast DeriveInput) -> Extraction<Self> {
        match Self::validate(node) {
            // Children keep their OWN extractions, reasons included. The parent does not absorb
            // them: a reason belongs where it was recorded, and the render walk collects them on
            // its way down (ID(no-ancestry)).
            Ok(data) => Extraction::value(Self {
                derive_input: node,
                fields: data.fields.iter().map(FieldExtraction::extract_from).collect(),
            }),
            Err(_) => Extraction::failed(Reason::new(ReasonKind::WrongShape, node)),
        }
    }
}

//TODO[ ](#extractor/error):R[S(StructExtractionValidityError) -> E(Reason)], "ANSWERED: meaning comes from a closed Reason set crossed with the Node reflection table, NOT from a taxonomy of error types. A proc macro never handles an error programmatically - it only emits one - so per-type errors buy nothing and actively fight accumulation, since two different error structs cannot combine. Collapse all four unit-struct error types here to syn::Error at the boundary"
pub struct StructExtractionValidityError;

impl<'ast> Validate<'ast, &'ast DeriveInput> for StructExtraction<'ast> {
    type ValidityError = StructExtractionValidityError;
    type Valid = &'ast DataStruct;

    fn validate(input: &'ast DeriveInput) -> Result<&'ast DataStruct, Self::ValidityError> {
        // `&input.data`, not `input.data` - matching by value moves the variant binding out and
        // the old `Ok(&data_struct)` handed back a reference to a local.
        match &input.data {
            syn::Data::Struct(data_struct) => Ok(data_struct),
            syn::Data::Enum(_) | syn::Data::Union(_) => Err(StructExtractionValidityError),
        }
    }
}
