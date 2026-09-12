// @review [x]
use std::error::Error;

// @review [ ]
// TODO(#extractor/pipeline):C[S(ExtractorPipeline)], "Bare struct holding its own extractor/processor/generator triple, mirroring the StructExtraction pipeline this file already builds - the extractor stage becomes self-hosting"
// TODO(#extractor/expansion):C[F(expand)], "expand(&ExtractorPipeline) -> TokenStream first, concretely; only then wire the outer expansion (ExtractionState<StructExtraction>::visit_derive_input over DeriveInput/ItemStruct). Two separate passes - don't conflate the inner macro-of-a-macro with the outer traversal already in processor.rs"
// TODO[~](#extractor/macro-wiring):U[F(extractor)], "Wire ExtractorPipeline::expand into lib.rs::extractor once it exists. Split from #extractor/macro so the two comments stop sharing one identity - nuts keys by identity, so a snapshot was only ever seeing one of them"
use syn::{DataStruct, DeriveInput, Fields, visit::Visit};

pub(crate) use crate::traits::extractor::ExtractionState;
use crate::{
    base::extractor::extractor::field::FieldExtraction,
    traits::{Validate, extractor::Extractor},
};
pub mod attribute;
pub mod field;

//Fix[ ](#extractor/recursive-source):U[Impl(Visit<'ast> for ExtractionState<StructExtraction<'ast>>)], "When expanding the Extractors, a macro should traverse from its OWN source type and find its children from there, never from a child's genesis syn type (Fields here). Renamed off #extractor/macro, which three comments were claiming at once. Note this is the OUTER syn traversal and is unrelated to the Meta/Expr walk in the syntax stage - keeping the two traversals distinct is the point of #extractor/expansion's 'two separate passes'"

pub(crate) struct StructExtraction<'ast> {
    pub(crate) fields: Vec<ExtractionState<FieldExtraction<'ast>>>,
}

pub struct ExtractionError;

impl<'ast> Extractor<'ast, &'ast DeriveInput> for StructExtraction<'ast> {
    type ExtractionError = ExtractionError;
    type Node = DeriveInput;

    fn extract_from(
        node: &'ast Self::Node,
    ) -> Result<ExtractionState<Self>, Self::ExtractionError> {
        if let Ok(ds) = Self::validate(node) {
            Ok(ExtractionState::Initialised(Self {
                fields: { ds.fields.iter().map(|f| FieldExtraction::extract_from(f)) },
            }))
        } else {
            Err(ExtractionError)
        }
    }
}

//TODO[ ](#extractor/error):R[S(StructExtractionValidityError) -> E(Reason)], "ANSWERED: meaning comes from a closed Reason set crossed with the Node reflection table, NOT from a taxonomy of error types. A proc macro never handles an error programmatically - it only emits one - so per-type errors buy nothing and actively fight accumulation, since two different error structs cannot combine. Collapse all four unit-struct error types here to syn::Error at the boundary"
pub struct StructExtractionValidityError;

impl<'ast> Validate<'ast, &'ast DeriveInput> for StructExtraction<'ast> {
    type ValidityError = StructExtractionValidityError;
    type Valid = &'ast DataStruct;

    fn validate(input: &'ast DeriveInput) -> Result<&'ast DataStruct, Self::ValidityError> {
        match input.data {
            syn::Data::Struct(data_struct) => Ok(&data_struct),
            syn::Data::Enum(data_enum) => Err(StructExtractionValidityError),
            syn::Data::Union(data_union) => Err(StructExtractionValidityError),
        }
    }
}
impl<'ast> Visit<'ast> for ExtractionState<StructExtraction<'ast>> {
    fn visit_fields(&mut self, i: &'ast Fields) {
        let fields = i
            .iter()
            .map(ExtractionState::<FieldExtraction<'_>>::extract)
            .collect();
        *self = ExtractionState::Initialised(StructExtraction { fields });
    }
}
