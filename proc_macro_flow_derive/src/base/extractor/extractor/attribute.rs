// @review [ ]
use syn::{Attribute, Expr, Meta};

use crate::traits::extractor::ExtractFrom;

use super::super::ExtractionState;

pub(crate) struct TransformationExtraction<'ast> {
    pub(crate) expression: &'ast Expr,
}

impl<'ast> ExtractFrom<'ast> for TransformationExtraction<'ast> {
    type Node = Attribute;

    fn extract_from(node: &'ast Attribute) -> ExtractionState<Self> {
        match &node.meta {
            Meta::NameValue(mnv) => ExtractionState::Initialised(TransformationExtraction {
                expression: &mnv.value,
            }),
            _ => ExtractionState::Uninitialised,
        }
    }
}
