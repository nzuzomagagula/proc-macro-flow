// @review [ ]
//! The concrete processor for the extractor stage: narrowing an extraction into what the
//! generator wants.
//!
//! Answer(#processor/base-scope):A[ID(processor/base-scope) ==? S(ProcessedStruct)], "The question
//! was what a base processor should VALIDATE or TRANSFORM before generation. Now answered by a real
//! one. It validates NOTHING - by the time an extraction arrives its reasons are recorded, and
//! re-checking would duplicate a test it cannot improve on while discarding the spans that make the
//! result diagnosable. It TRANSFORMS: Extracted<StructExtraction, &DeriveInput> becomes
//! ProcessedStruct, which holds the two things code generation actually needs - the item to name,
//! and its fields. Everything the extraction carried that generation does not need is dropped here,
//! which is the whole job"
//!
//! NOTE(#processor/needs-the-source): V[F(process).uses(F(Extracted::source))], "This is the
//! clearest demonstration of why the source rides on the OUTPUT. FieldExtraction currently carries
//! NO data at all - its value is an empty struct - so everything ProcessedField knows comes from
//! `input.source()`, the node the extraction was read from. Had the pipeline unwrapped Extracted
//! between stages, or had the source lived on the value through Tr(Sourced), a processor would have
//! nothing to work with the moment an extraction failed. It is available here precisely because it
//! never depended on the value existing"

use proc_macro_flow_traits::{extractor::Extracted, extractor::Extraction, processor::Processor};
use syn::{DeriveInput, Field};

use crate::base::extractor::extractor::{field::FieldExtraction, StructExtraction};

// DEPRECATED(#processor/bare-source):D[S(ExtractorProcessor)] && D[S(FieldProcessor)] && D[S(TransformationProcessor)], "Deleted. Each held `source: XExtraction` - the bare extraction, unwrapped - and that is settled the other way: a processor receives Ty(Extractor::Output) WHOLE. Unwrapping would strip the source node off exactly the value a processor needs it for. Replaced by the impls below, which take Extracted and narrow it"
// TODO(#processor/macro):C[F(processor)], "Proc-macro entry point for the processor stage, alongside lib.rs::field_names (ID(extractor/macro-wiring))"

/// What the generator consumes for a whole struct.
pub(crate) struct ProcessedStruct<'ast> {
    /// The item being derived on - what generation needs to name the impl.
    pub(crate) item: &'ast DeriveInput,
    pub(crate) fields: Vec<ProcessedField<'ast>>,
}

/// What the generator consumes per field.
pub(crate) struct ProcessedField<'ast> {
    pub(crate) field: &'ast Field,
}

impl<'ast> Processor for FieldExtraction<'ast> {
    type Input = Extracted<FieldExtraction<'ast>, &'ast Field>;
    type Output = ProcessedField<'ast>;

    fn process(input: Self::Input) -> Extraction<Self::Output> {
        // The node first: it is available whether or not the extraction produced a value, which is
        // the point of ID(processor/needs-the-source).
        let field = *input.source();
        let extraction = input.into_extraction();

        Extraction {
            value: extraction.value.map(|_| ProcessedField { field }),
            reasons: extraction.reasons,
        }
    }
}

impl<'ast> Processor for StructExtraction<'ast> {
    type Input = Extracted<StructExtraction<'ast>, &'ast DeriveInput>;
    type Output = ProcessedStruct<'ast>;

    fn process(input: Self::Input) -> Extraction<Self::Output> {
        let item = *input.source();
        let extraction = input.into_extraction();

        let mut out: Extraction<ProcessedStruct<'ast>> = Extraction {
            value: None,
            reasons: extraction.reasons,
        };

        // A failed extraction still carries its reasons upward - nothing is dropped just because
        // there is no value to narrow.
        let Some(value) = extraction.value else {
            return out;
        };

        // CHILDREN FIRST, then combine. `absorb` takes each child's reasons across whether or not
        // it produced a value, so a bad field cannot silently remove its siblings' complaints.
        let mut fields = Vec::new();
        for child in FieldExtraction::process_each(value.fields) {
            if let Some(field) = out.absorb(child) {
                fields.push(field);
            }
        }

        out.value = Some(ProcessedStruct { item, fields });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro_flow_traits::extractor::Extractor;
    use syn::parse_str;

    fn processed(source: &str) -> Extraction<ProcessedStruct<'_>> {
        // leak so the borrow outlives the call, which a real macro gets for free from its input
        let input: &'static DeriveInput =
            Box::leak(Box::new(parse_str(source).expect("the item parses")));
        StructExtraction::process(StructExtraction::extract_from(input))
    }

    #[test]
    fn a_struct_narrows_to_what_generation_needs() {
        let out = processed("pub struct Thing { a: u8, b: String }");
        let value = out.value.expect("a struct extracts");

        assert_eq!(value.item.ident.to_string(), "Thing");
        assert_eq!(value.fields.len(), 2);
        assert_eq!(
            value.fields[0].field.ident.as_ref().unwrap().to_string(),
            "a"
        );
    }

    #[test]
    fn a_field_knows_only_what_its_source_told_it() {
        // ID(processor/needs-the-source): FieldExtraction's value is empty, so everything here
        // came from Extracted::source. Nothing would be left if the pipeline had unwrapped.
        let out = processed("pub struct Thing { named: u8 }");
        let value = out.value.unwrap();

        assert_eq!(
            value.fields[0].field.ident.as_ref().unwrap().to_string(),
            "named"
        );
    }

    #[test]
    fn a_tuple_struct_has_unnamed_fields_and_still_processes() {
        let out = processed("pub struct Thing(u8, String);");
        let value = out.value.expect("tuple structs extract");

        assert_eq!(value.fields.len(), 2);
        assert!(value.fields[0].field.ident.is_none());
    }

    #[test]
    fn an_enum_fails_extraction_and_the_reason_survives_processing() {
        // StructExtraction::validate rejects a non-struct. The processor has no value to narrow,
        // and must still carry the complaint upward rather than swallowing it.
        let out = processed("pub enum Thing { A, B }");

        assert!(out.value.is_none());
        assert_eq!(out.reasons.len(), 1);
    }

    #[test]
    fn a_struct_with_no_fields_processes_to_an_empty_list() {
        let out = processed("pub struct Thing;");
        assert!(out.value.unwrap().fields.is_empty());
    }
}
