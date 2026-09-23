// @review [ ]
//! The concrete processor for the extractor stage: narrowing an extraction into what the
//! generator wants.
//!
//! NOTE(#processor/base-scope): a processor VALIDATES NOTHING and TRANSFORMS ONLY. By the time an
//! extraction arrives its reasons are recorded, and re-checking would discard the spans that make them
//! diagnosable.

use proc_macro2::TokenStream;
use proc_macro_flow_traits::{
    extractor::Extracted,
    extractor::Extraction,
    processor::Processor,
    resolution::{Deferred, Raw},
};
use syn::{Attribute, DeriveInput, Field};

use crate::base::extractor::extractor::{field::FieldExtraction, StructExtraction};
use crate::base::syntax::extractor::{
    SyntaxFieldAttributeExtraction, SyntaxFieldAttributeKind, SyntaxHelper,
};


/// What the generator consumes for a whole struct.
pub(crate) struct ProcessedStruct<'ast> {
    /// The item being derived on - what generation needs to name the impl.
    pub(crate) item: &'ast DeriveInput,
    pub(crate) fields: Vec<ProcessedField<'ast>>,
}

/// What the generator consumes per field.
pub(crate) struct ProcessedField<'ast> {
    pub(crate) field: &'ast Field,
    /// The field's grammar attributes, processed.
    ///
    /// Empty until ID(field/children) made a field's attributes its children; before that this
    /// struct was the bottom of the pipeline and processing stopped a level short of extraction.
    pub(crate) attrs: Vec<ProcessedAttribute<'ast>>,
}

/// What the generator consumes per helper attribute.
///
pub(crate) struct ProcessedAttribute<'ast> {
    // NOTE(#processed-attribute/no-unread-node): V[!S(ProcessedAttribute).P(attribute)], "This
    // struct deliberately does NOT carry its `&'ast Attribute`. It did for one commit, on the
    // reasoning that ID(typed-output/spans) will eventually want a node to span generated errors
    // against - and that is exactly the reasoning that produced Tr(Sourced), a whole trait making
    // every extractor store and hand back a node that one test read. The crate's standard is that
    // unread is unread: ID(extraction/unconsumed) took an allow OFF a field the moment it got real
    // readers, rather than suppressing the warning while waiting for one. When
    // ID(typed-output/spans) lands it can add the node back WITH a reader, which is a smaller and
    // more honest change than keeping a field warm for a year"
    /// Which helper this is, resolved once at `validate` and never compared again.
    pub(crate) helper: SyntaxHelper,
    /// The argument tokens, still UNREAD. Carrying them is the whole point - ID(no-parse) - and
    /// processing them here would be the stage boundary violation the design exists to prevent.
    pub(crate) tokens: &'ast TokenStream,
}

/// The attribute stage's processor.
///
/// Lives here rather than beside the extraction in `base/syntax` because what it produces is
/// consumed by THIS pipeline's generator - it is the extractor pipeline's third level, not a
/// separate stage. The orphan rule permits either; cohesion picks this one.
impl<'ast> Processor for SyntaxFieldAttributeExtraction<'ast, Raw> {
    type Input = Extracted<Self, &'ast Attribute>;
    type Output = ProcessedAttribute<'ast>;

    fn process(input: Self::Input) -> Extraction<Self::Output> {
        let extraction = input.into_extraction();

        Extraction {
            value: extraction.value.map(|node| {
                // Exhaustive over the kind, matching `extract_from`'s own rule: a new helper stops
                // compiling here rather than silently producing nothing.
                let (helper, tokens) = match node.kind() {
                    SyntaxFieldAttributeKind::Shape(shape) => (SyntaxHelper::Shape, shape.tokens()),
                    SyntaxFieldAttributeKind::Alias(alias) => (SyntaxHelper::Alias, alias.tokens()),
                };

                ProcessedAttribute { helper, tokens }
            }),
            reasons: extraction.reasons,
        }
    }
}

impl<'ast> Processor for FieldExtraction<'ast> {
    type Input = Extracted<FieldExtraction<'ast>, &'ast Field>;
    type Output = ProcessedField<'ast>;

    fn process(input: Self::Input) -> Extraction<Self::Output> {
        // The node first: it is available whether or not the extraction produced a value, which is
        // the point of ID(processor/needs-the-source).
        let field = *input.source();
        let extraction = input.into_extraction();

        let mut out: Extraction<ProcessedField<'ast>> = Extraction {
            value: None,
            reasons: extraction.reasons,
        };

        let Some(value) = extraction.value else {
            return out;
        };

        // CHILDREN FIRST, then combine - the same shape StructExtraction::process uses. This loop
        // is what the pipeline was missing: the attributes were extracted and walked for reasons,
        // then dropped on the way to generation by a `.map(|_| ..)` that ignored the value.
        let mut attrs = Vec::new();
        for child in SyntaxFieldAttributeExtraction::process_each(value.attrs) {
            if let Some(attr) = out.absorb(child) {
                attrs.push(attr);
            }
        }

        out.value = Some(ProcessedField { field, attrs });
        out
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

    #[test]
    fn a_fields_attributes_survive_processing() {
        // ID(field/children) made them extraction's children; this is where they used to stop.
        let out = processed("pub struct Thing { #[shape(AttributeKind::MetaList)] a: u8 }");
        let value = out.value.expect("a struct extracts");

        let attrs = &value.fields[0].attrs;
        assert_eq!(attrs.len(), 1, "the attribute was dropped by the processor");
        assert_eq!(attrs[0].helper, SyntaxHelper::Shape);
        assert_eq!(attrs[0].tokens.to_string(), "AttributeKind :: MetaList");
    }

    #[test]
    fn processing_narrows_and_does_not_resolve() {
        let out = processed("pub struct Thing { #[alias(colours)] a: u8 }");
        let value = out.value.unwrap();

        assert_eq!(value.fields[0].attrs[0].helper, SyntaxHelper::Alias);
        assert_eq!(value.fields[0].attrs[0].tokens.to_string(), "colours");
    }

    #[test]
    fn a_doc_comment_processes_to_nothing_and_says_nothing() {
        // The child is carried by extraction but has no value, so it narrows to nothing here -
        // and records no complaint on the way. ID(heads-are-rustcs).
        let out = processed("pub struct Thing { /// documented\n a: u8 }");
        let value = out.value.unwrap();

        assert!(value.fields[0].attrs.is_empty());
        assert!(out.reasons.is_empty(), "a doc comment was complained about");
    }
}
