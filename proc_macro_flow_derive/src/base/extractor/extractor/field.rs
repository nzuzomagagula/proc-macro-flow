// @review [ ]
use proc_macro_flow_traits::{
    extractor::{Extracted, Extraction},
    source::Sourced,
};
use syn::{Attribute, Field};

use crate::base::extractor::extractor::attribute::TransformationExtraction;
use crate::traits::Validate;
use crate::traits::extractor::{Extractor, extract_each};

pub(crate) struct FieldExtraction<'ast> {
    pub(crate) field: &'ast Field,
    pub(crate) transformation: Vec<Extracted<TransformationExtraction<'ast>, &'ast Attribute>>,
}

pub struct FieldExtractionError;

impl<'ast> Sourced<'ast> for FieldExtraction<'ast> {
    type Source = Field;

    fn source(&self) -> &'ast Field {
        self.field
    }
}

impl<'ast> Extractor<'ast, &'ast Field> for FieldExtraction<'ast> {
    type Output = Extracted<Self, &'ast Field>;

    fn extract_from(node: &'ast Field) -> Self::Output {
        // `validate` is infallible today, so this cannot currently produce a Reason of its own -
        // every complaint comes from a child. That changes with ID(syntax/extraction).
        let extraction = match Self::validate(node) {
            // Same shape, same reason - `#[from = source.attrs]`. Arity comes off the Vec,
            // never off the attribute.
            Ok(field) => Extraction::value(Self {
                field,
                transformation: extract_each::<TransformationExtraction, _, _>(field.attrs.iter()),
            }),
            Err(_) => Extraction::default(),
        };

        Extracted::new(extraction, node)
    }
}

impl<'ast> Validate<'ast, &'ast Field> for FieldExtraction<'ast> {
    type ValidityError = FieldExtractionError;

    type Valid = &'ast Field;

    // TODO[ ](#extractor/field-validate):U[F(validate)], "Validates nothing - every Field is
    // accepted. Kept honest rather than made to look busy: what there is to check here is whether
    // the field's attributes form a well-shaped grammar node, and that is ID(syntax/extraction)'s
    // job, not a check this stage can do on its own"
    fn validate(input: &'ast Field) -> Result<Self::Valid, Self::ValidityError> {
        Ok(input)
    }
}
