// @review [ ]
// Answer(#extractor/self-hosting):A[S(ExtractorPipeline) == Attr(derive(Extractor))], "THE SAME
// FEATURE, ARRIVED AT BY ANOTHER ROUTE. @group(#extractor/self-hosting) asked for a bare
// ExtractorPipeline struct holding its own extractor/processor/generator triple, an expand() over
// it, and that expansion wired into the macro entry point - 'the extractor stage becomes
// self-hosting'. What shipped answers the same question declaratively: Attr(source) names the syn
// node, Attr(from)/Attr(with) name where each child comes from, and Attr(derive(Extractor)) reads
// them off the extraction type itself. Nothing holds a triple, because nothing needs to: the field
// TYPE already carries the arity (ID(from/arity-from-type)) and Ty(Extracted) already carries the
// source, so a struct assembled to describe a pipeline would only be restating what the extraction
// struct says. The three below are closed against this answer, not abandoned"
//
// TODO[x](#extractor/pipeline):C[S(ExtractorPipeline)], "CLOSED by ID(extractor/self-hosting) - and
// the type is deliberately NOT built. Its job was to be the thing a macro expands; the derive
// expands the extraction struct instead, which is one fewer type declaring the same shape twice"
// TODO[x](#extractor/expansion):C[F(expand)], "CLOSED by ID(extractor/self-hosting). The split it
// insisted on - inner macro-of-a-macro first, outer traversal second - stopped applying when the
// outer traversal went away: ID(extractor/recursive-source) deleted the Visit walk, so extract_from
// descends from its own source and there is no second pass to conflate with"
// TODO[x](#extractor/macro-wiring):U[F(field_names)], "CLOSED by ID(extractor/self-hosting). There
// is no expand() to wire, and the target it named moved: lib.rs::extractor is now F(field_names),
// while F(extractor) is the DERIVE. VERIFIED that the rename was forced rather than chosen - two
// Attr(proc_macro_derive(Extractor)) in one crate is `error[E0428]: the name Extractor is defined
// multiple times`. Recorded because this annotation resolved cleanly to the wrong function for a
// while, which is worse than dangling"
use syn::{DataStruct, DeriveInput, Field};

pub(crate) use proc_macro_flow_traits::extractor::{Extracted, Extraction};
use proc_macro_flow_traits::extractor::{Extractor, Reason, ReasonKind, Validate};
use proc_macro_flow_traits::render::Diagnose;

use crate::base::extractor::extractor::field::FieldExtraction;

pub mod field;

//Fix[x](#extractor/recursive-source):D[Impl(Visit<'ast> for ExtractionState<StructExtraction<'ast>>)], "RESOLVED by deletion, not by rewiring. The objection was that a macro should traverse from its OWN source type and find its children from there, never from a child's genesis syn type - and extract_from now does exactly that: it takes the DeriveInput, validates it to a DataStruct, and maps its fields. The Visit impl walked from Fields, could not name a source, and only ever reached the right node by falling through syn's default traversal. Two further reasons not to keep it: Extraction lives in proc_macro_flow_traits now, so impl Visit for it is an orphan-rule violation, and the visitor could not satisfy Sourced. The OUTER-vs-Meta/Expr distinction the note drew still holds and is ID(extractor/expansion)'s business"

pub(crate) struct StructExtraction<'ast> {
    // Fix[x](#extraction/unconsumed):D[Attr(allow(dead_code))], "RESOLVED. The children used to be
    // extracted and then never looked at, which is why this field carried a suppression and a
    // warning named after it. Two readers arrived: ID(pipeline/base-processor) narrows them for
    // generation, and ID(syntax/render) walks them for their reasons. The allow comes off - if
    // either reader is ever removed the warning should come back rather than stay silenced"
    pub(crate) fields: Vec<Extracted<FieldExtraction<'ast>, &'ast Field>>,
}

impl<'ast> Extractor<'ast> for StructExtraction<'ast> {
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
                fields: FieldExtraction::extract_each(data.fields.iter()),
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

impl<'ast> Validate<'ast> for StructExtraction<'ast> {
    type Source = &'ast DeriveInput;
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

impl<'ast> Diagnose for StructExtraction<'ast> {
    /// Only where the children are - the `Extracted` around each one renders its reasons, because
    /// it is the only thing that knows the node they span against. See NOTE(#render/who-renders).
    fn diagnose(&self, out: &mut Vec<syn::Error>) {
        self.fields.diagnose(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro_flow_traits::render::Diagnose;
    use syn::parse_str;

    fn item(source: &str) -> DeriveInput {
        parse_str(source).expect("the item parses")
    }

    /// Stand in for a reason a field records once ID(field/children) gives it something to
    /// complain about. The walk has to reach it today, or it will not reach the real one either.
    fn with_a_complaining_field<'ast>(
        node: &'ast DeriveInput,
        field: &'ast Field,
    ) -> Extracted<StructExtraction<'ast>, &'ast DeriveInput> {
        let child = Extracted::new(
            FieldExtraction::extract_from(field)
                .into_extraction()
                .with_reason(Reason::new(ReasonKind::UnknownKey)),
            field,
        );

        Extracted::new(
            Extraction::value(StructExtraction {
                fields: vec![child],
            }),
            node,
        )
    }

    #[test]
    fn a_clean_struct_renders_nothing() {
        let input = item("pub struct Thing { a: u8, b: String }");
        assert!(StructExtraction::extract_from(&input).render().is_empty());
    }

    #[test]
    fn a_failed_validate_renders_the_roots_own_reason() {
        let input = item("pub enum Thing { A }");
        let errors = StructExtraction::extract_from(&input).render();

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "written in the wrong shape");
    }

    #[test]
    fn a_fields_reason_reaches_the_walk() {
        // THE wiring this file owed. StructExtraction::diagnose says where its children are and
        // the Extracted around each one renders it - neither half works alone.
        let input = item("pub struct Thing { a: u8 }");
        let data = match &input.data {
            syn::Data::Struct(data) => data,
            _ => unreachable!(),
        };
        let field = data.fields.iter().next().expect("one field");

        let errors = with_a_complaining_field(&input, field).render();

        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "not a key this node accepts");
    }

    #[test]
    fn the_reason_is_spanned_against_the_field_not_the_item() {
        // ID(reason/span-not-node): the reason had no span of its own, so it fell back to the node
        // its Extracted holds - the FIELD. Falling back to the DeriveInput would underline the
        // whole struct for a one-field complaint.
        let input = item("pub struct Thing { a: u8 }");
        let syn::Data::Struct(data) = &input.data else {
            unreachable!()
        };
        let field = data.fields.iter().next().expect("one field");

        let errors = with_a_complaining_field(&input, field).render();
        let rendered = errors[0].to_compile_error().to_string();

        // spanned output is not inspectable on stable (ID(render/traversal-is-source-order) covers
        // the same limit), so assert what IS observable: one error, carrying the child's wording
        assert!(rendered.contains("compile_error"), "{rendered}");
        assert!(rendered.contains("not a key"), "{rendered}");
    }

    #[test]
    fn a_source_that_pins_nothing_still_resolves() {
        // REGRESSION for NOTE(#pipeline/source-is-associated). While the source was a trait
        // PARAMETER these two shapes failed with `error[E0283]: type annotations needed` the
        // moment a type had more than one impl - and both are ordinary things for the derive to
        // emit for an Option field or an empty iterator. Neither argument pins the source; the
        // associated type does, because it is determined by Self.
        //
        // No turbofish, no annotation. That is the whole assertion - if this file stops compiling
        // the source has drifted back to being guessed.
        assert!(FieldExtraction::extract_maybe(None).is_none());
        assert!(FieldExtraction::extract_each(::core::iter::empty()).is_empty());
    }
}
