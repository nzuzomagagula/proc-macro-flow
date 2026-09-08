// @review [ ]
use syn::{Attribute, Expr, Meta};

use crate::traits::{Validate, extractor::Extractor};

use super::super::ExtractionState;

// TODO(#attribute/helpers):C[Tr(AttributePattern)], "Formalise attribute-parsing helpers instead of hand-matching Meta variants per extraction type - a syn-like combinator library isn't needed, attributes here only ever take one of the few fixed shapes below"
// TODO(#attribute/list):C[F(parse_list)], "Macro-list pattern #[attr(a, b, c)] maps to an enum - one variant per allowed ident/path in the list"
// TODO(#attribute/path):C[F(parse_path)], "Bare-path pattern #[attr] / #[attr::marker] maps to a ZST/unit struct - no payload, presence is the signal"
// TODO(#attribute/name-value):C[F(parse_name_value)], "Name-value pattern #[attr(key = val, ..)] maps to a struct (named fields) or tuple (positional) - TransformationExtraction::extract_from below only handles a single top-level Meta::NameValue today"
pub(crate) struct TransformationExtraction<'ast> {
    pub(crate) expression: &'ast Expr,
}
pub struct TransformationExtractionError;

impl<'ast> Extractor<'ast, &'ast Attribute> for TransformationExtraction<'ast> {
    type Node = Attribute;

    type ExtractionError = TransformationExtractionError;

    fn extract_from(
        node: &'ast Self::Node,
    ) -> Result<ExtractionState<Self>, Self::ExtractionError> {
        todo!()
    }
}

impl<'ast> Validate<'ast, &'ast Attribute> for TransformationExtraction<'ast> {
    type ValidityError = TransformationExtractionError;

    type Valid = &'ast Attribute;

    //TODO[~](#i-was-busy/finish-this):U[this, "Finish this"]
    fn validate(input: &'ast Attribute) -> Result<Self::Valid, Self::ValidityError> {}
}
