// @review [ ]
use syn::{Field, visit::Visit};

use crate::base::extractor::extractor::attribute::TransformationExtraction;

use super::super::ExtractionState;
pub(crate) struct FieldExtraction<'ast> {
    pub(crate) field: &'ast Field,
    pub(crate) transformation: Vec<ExtractionState<TransformationExtraction<'ast>>>,
}

impl<'ast> Visit<'ast> for ExtractionState<FieldExtraction<'ast>> {
    fn visit_field(&mut self, i: &'ast Field) {
        let transformation = i
            .attrs
            .iter()
            .map(|att| {
                let mut te = ExtractionState::<TransformationExtraction>::Uninitialised;
                te.visit_attribute(att);
                te
            })
            .collect();
        *self = ExtractionState::Initialised(FieldExtraction {
            field: i,
            transformation,
        });
    }
}
