// @review [ ]
use syn::Field;

use crate::base::extractor::extractor::attribute::TransformationExtraction;
use crate::traits::Validate;
use crate::traits::extractor::Extractor;

use super::super::ExtractionState;
pub(crate) struct FieldExtraction<'ast> {
    pub(crate) field: &'ast Field,
    pub(crate) transformation: Vec<ExtractionState<TransformationExtraction<'ast>>>,
}
pub struct FieldExtractionError;
impl<'ast> Extractor<'ast, &'ast Field> for FieldExtraction<'ast> {
    type Node = Field;

    type ExtractionError = FieldExtractionError;

    fn extract_from(
        node: &'ast Self::Node,
    ) -> Result<ExtractionState<Self>, Self::ExtractionError> {
        if let Ok(f) = Self::validate(node) {
            Ok(ExtractionState::Initialised(Self {
                field: f,
                transformation: f
                    .attrs
                    .iter()
                    .map(|att| TransformationExtraction::extract_from(att)),
            }))
        } else {
            Err(FieldExtractionError)
        }
    }
}

impl<'ast> Validate<'ast, &'ast Field> for FieldExtraction<'ast> {
    type ValidityError = FieldExtractionError;

    type Valid = &'ast Field;

    fn validate(input: &'ast Field) -> Result<Self::Valid, Self::ValidityError> {
        Ok(input)
    }
}
