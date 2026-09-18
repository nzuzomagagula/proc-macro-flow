// @review [ ]
// TODO(#extractor/pipeline):C[S(ExtractorPipeline)], "Bare struct holding its own extractor/processor/generator triple, mirroring the StructExtraction pipeline this file already builds - the extractor stage becomes self-hosting"
// TODO(#extractor/expansion):C[F(expand)], "expand(&ExtractorPipeline) -> TokenStream first, concretely; only then wire the outer expansion (ExtractionState<StructExtraction>::visit_derive_input over DeriveInput/ItemStruct). Two separate passes - don't conflate the inner macro-of-a-macro with the outer traversal already in processor.rs"
// TODO[~](#extractor/macro-wiring):U[F(extractor)], "Wire ExtractorPipeline::expand into lib.rs::extractor once it exists. Split from #extractor/macro so the two comments stop sharing one identity - nuts keys by identity, so a snapshot was only ever seeing one of them"
use syn::{DataStruct, DeriveInput, Field};

pub(crate) use proc_macro_flow_traits::extractor::{Extracted, Extraction};
use proc_macro_flow_traits::extractor::{Reason, ReasonKind};
use crate::{
    base::extractor::extractor::field::FieldExtraction,
    traits::{
        Validate,
        extractor::{Extractor, extract_each},
    },
};
pub mod field;

//Fix[x](#extractor/recursive-source):D[Impl(Visit<'ast> for ExtractionState<StructExtraction<'ast>>)], "RESOLVED by deletion, not by rewiring. The objection was that a macro should traverse from its OWN source type and find its children from there, never from a child's genesis syn type - and extract_from now does exactly that: it takes the DeriveInput, validates it to a DataStruct, and maps its fields. The Visit impl walked from Fields, could not name a source, and only ever reached the right node by falling through syn's default traversal. Two further reasons not to keep it: Extraction lives in proc_macro_flow_traits now, so impl Visit for it is an orphan-rule violation, and the visitor could not satisfy Sourced. The OUTER-vs-Meta/Expr distinction the note drew still holds and is ID(extractor/expansion)'s business"

pub(crate) struct StructExtraction<'ast> {
    // UNWIRED(#extraction/unconsumed): V[this.built && !this.read], "The children are
    // extracted and then nobody looks at them - the processor that would is ID(pipeline/base-processor),
    // still a stub. This is the single most load-bearing warning in the crate, so it is
    // suppressed HERE and named rather than left to blend into the noise."
    #[allow(dead_code)]
    pub(crate) fields: Vec<Extracted<FieldExtraction<'ast>, &'ast Field>>,
}

impl<'ast> Extractor<'ast, &'ast DeriveInput> for StructExtraction<'ast> {
    type Output = Extracted<Self, &'ast DeriveInput>;

    fn extract_from(node: &'ast DeriveInput) -> Self::Output {
        let extraction = match Self::validate(node) {
            // Children keep their OWN extractions, reasons included. The parent does not absorb
            // them: a reason belongs where it was recorded, and the render walk collects them on
            // its way down (ID(no-ancestry)).
            // The shape `#[from = source.data.fields]` will generate: the designer names where
            // the children are, the Vec in the field's type picks `extract_each`, and the walk is
            // not written out. See @group(#from).
            Ok(data) => Extraction::value(Self {
                fields: extract_each::<FieldExtraction, _, _>(data.fields.iter()),
            }),
            Err(_) => Extraction::failed(Reason::new(ReasonKind::WrongShape)),
        };

        // The source rides on the OUTPUT and is stored nowhere else. It survives the Err arm
        // above, where there is no Self to ask at all - which is why Tr(Sourced) was redundant.
        // See ID(extracted/source-when-absent).
        Extracted::new(extraction, node)
    }
}

//TODO[ ](#extractor/error):R[S(StructExtractionValidityError) -> E(Reason)], "ANSWERED but NOT YET DONE, and the two halves have come apart. The answer stands: meaning comes from a closed Reason set, not from a taxonomy of error types - a proc macro never handles an error programmatically, it only emits one, so per-type errors buy nothing and cannot combine with a sibling's. What has changed is where they survive. extract_from no longer has an ExtractionError at all (ID(extractor/no-result)), so the four remaining unit structs - StructExtractionValidityError, FieldExtractionError, TransformationExtractionError, SyntaxFieldAttributeError - are now Tr(Validate)::ValidityError and nothing else. Collapsing them is therefore a VALIDATE question: under ID(pipeline/validity-scope) a surface check has a span and a cause and nothing more, which is a Reason, so ValidityError should stop being an associated type rather than becoming syn::Error"
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
