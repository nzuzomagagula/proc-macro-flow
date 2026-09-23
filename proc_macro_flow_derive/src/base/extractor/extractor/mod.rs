// @review [ ]
// NOTE(#extractor/pipeline): S(ExtractorPipeline) names the three stages so the BOUNDS make them
// agree, and holds no data of its own. If it ever grows a field restating the extraction, that is the
// drift to undo.
// NOTE(#extractor/macro-wiring): the entry point is F(field_names); F(extractor) is the DERIVE. Two
// Attr(proc_macro_derive(Extractor)) in one crate is `error[E0428]`, which forced the split.
use syn::{DataStruct, DeriveInput, Field};

pub(crate) use proc_macro_flow_traits::extractor::{Extracted, Extraction};
use proc_macro_flow_traits::extractor::{Extractor, Reason, ReasonKind, Validate};
use proc_macro_flow_traits::render::Diagnose;

use crate::base::extractor::extractor::field::FieldExtraction;

pub mod field;

// NOTE(#extractor/recursive-source): F(extract_from) descends from its OWN source, never from a
// child's genesis type. There is no Visit walk.

pub(crate) struct StructExtraction<'ast> {
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

// NOTE(#extractor/error): meaning comes from the closed E(ReasonKind) set, not a taxonomy of error
// types. Ty(Validate::ValidityError) is gone - it was written by five impls and read by none.

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
