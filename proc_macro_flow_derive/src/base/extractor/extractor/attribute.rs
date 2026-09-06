// @review [ ]
use syn::{Attribute, Expr, Meta, visit::Visit};

use super::super::ExtractionState;

pub(crate) struct TransformationExtraction<'ast> {
    pub(crate) expression: &'ast Expr,
}

impl<'ast> Visit<'ast> for ExtractionState<TransformationExtraction<'ast>> {
    fn visit_attribute(&mut self, i: &'ast Attribute) {
        if let Meta::NameValue(mnv) = &i.meta {
            *self = ExtractionState::Initialised(TransformationExtraction {
                expression: &mnv.value,
            });
        }
    }
}
