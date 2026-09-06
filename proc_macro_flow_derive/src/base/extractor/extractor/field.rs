// @review [ ]
use syn::Field;

use crate::base::extractor::extractor::attribute::TransformationExtraction;
use crate::traits::extractor::Extractor;

use super::super::ExtractionState;
pub(crate) struct FieldExtraction<'ast> {
    pub(crate) field: &'ast Field,
    pub(crate) transformation: Vec<ExtractionState<TransformationExtraction<'ast>>>,
}

impl<'ast> Extractor<'ast> for FieldExtraction<'ast> {
    type Node = Field;

    fn extract_from(node: &'ast Field) -> ExtractionState<Self> {
        let transformation = node
            .attrs
            .iter()
            .map(|att| ExtractionState::<TransformationExtraction<'_>>::extract(att))
            .collect();
        ExtractionState::Initialised(FieldExtraction {
            field: node,
            transformation,
        })
    }
}
