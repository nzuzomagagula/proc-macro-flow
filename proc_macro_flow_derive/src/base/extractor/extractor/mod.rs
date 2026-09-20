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
            // validate's reason is RECORDED here - see NOTE(#validate/reason-is-offered-not-imposed)
            // for why that is a choice this function makes rather than something the signature
            // forces.
            Err(reason) => Extraction::failed(reason),
        };

        // The source rides on the OUTPUT and is stored nowhere else. It survives the Err arm
        // above, where there is no Self to ask at all - which is why Tr(Sourced) was redundant.
        // See ID(extracted/source-when-absent).
        Extracted::new(extraction, node)
    }
}

// TODO[x](#extractor/error):R[Ty(Validate::ValidityError) -> E(Reason)], "DONE. The answer was the
// one predicted - meaning comes from a closed Reason set, not a taxonomy of error types - and the
// three unit structs it named are deleted: StructExtractionValidityError, FieldExtractionError and
// SyntaxFieldAttributeError. A fourth, TransformationExtractionError, had already gone with
// ID(attribute/generic-grammar), so the annotation was describing a type that no longer existed.
//
// What made the case unarguable was evidence rather than argument. The associated type was written
// by five impls and read by ZERO - every Err arm discarded it - while three of the five already set
// it to `()`. Two things fell out of removing it that were not the point but are worth more than
// the tidying: validate now builds its reason WHERE THE CAUSE IS KNOWN, so this extraction reports
// against the offending ident instead of extract_from hardcoding WrongShape against the whole item;
// and the derive stopped failing SILENTLY (ID(derive/silent-validate))"

impl<'ast> Validate<'ast> for StructExtraction<'ast> {
    type Source = &'ast DeriveInput;
    type Valid = &'ast DataStruct;

    fn validate(input: &'ast DeriveInput) -> Result<&'ast DataStruct, Reason> {
        // `&input.data`, not `input.data` - matching by value moves the variant binding out and
        // the old `Ok(&data_struct)` handed back a reference to a local.
        match &input.data {
            syn::Data::Struct(data_struct) => Ok(data_struct),
            // The reason is built HERE now, where the cause is known, instead of extract_from
            // discarding an opaque error type and hardcoding one. It carries a span of its own:
            // the ident is what the author can act on, not the whole item.
            syn::Data::Enum(_) | syn::Data::Union(_) => {
                Err(Reason::at(ReasonKind::WrongShape, &input.ident))
            }
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

    #[test]
    fn a_fields_attributes_are_its_children() {
        // ID(field/children). Before this the field was a leaf and its attributes were never
        // looked at by anything on the macro path.
        let input = item("pub struct Thing { #[shape(AttributeKind::MetaList)] a: u8 }");
        let extracted = StructExtraction::extract_from(&input);

        let field = &extracted.value().unwrap().fields[0];
        assert_eq!(field.value().unwrap().attrs.len(), 1);
    }

    #[test]
    fn a_grammar_attribute_extracts_a_value() {
        let input = item("pub struct Thing { #[shape(AttributeKind::MetaList)] a: u8 }");
        let extracted = StructExtraction::extract_from(&input);

        let field = &extracted.value().unwrap().fields[0];
        let attr = &field.value().unwrap().attrs[0];
        assert!(attr.value().is_some(), "the shape attribute did not extract");
        assert!(attr.reasons().is_empty());
    }

    #[test]
    fn a_doc_comment_is_not_an_error() {
        // REGRESSION for ID(heads-are-rustcs), and the first time it is on the macro path where a
        // regression would actually reach users. A doc comment IS an attribute, so the naive
        // reading - unknown head, therefore UnknownKey - makes every documented field an error.
        // That was a real bug once.
        //
        // The child is still PRESENT, carrying no value and no reason: extraction carries and does
        // not judge. What matters is that the walk emits nothing.
        let input = item("pub struct Thing { /// documented\n a: u8 }");
        let extracted = StructExtraction::extract_from(&input);

        let field = &extracted.value().unwrap().fields[0];
        let attrs = &field.value().unwrap().attrs;

        assert_eq!(attrs.len(), 1, "the doc attribute was dropped, not carried");
        assert!(attrs[0].value().is_none());
        assert!(attrs[0].reasons().is_empty(), "a doc comment was complained about");
        assert!(extracted.render().is_empty(), "a documented field emitted an error");
    }

    #[test]
    fn a_malformed_grammar_attribute_does_complain() {
        // The other side of the same rule: the head IS ours, so the shape is our business.
        let input = item("pub struct Thing { #[shape] a: u8 }");
        let extracted = StructExtraction::extract_from(&input);

        let errors = extracted.render();
        assert_eq!(errors.len(), 1, "a bare `shape` head went unreported");
        assert_eq!(errors[0].to_string(), "written in the wrong shape");
    }

    #[test]
    fn a_childs_reason_climbs_two_levels_to_the_walk() {
        // struct -> field -> attribute, all generated by hand here, and the walk crosses both.
        let input = item("pub struct Thing { #[shape] a: u8, #[shape] b: u8 }");
        assert_eq!(StructExtraction::extract_from(&input).render().len(), 2);
    }
}
