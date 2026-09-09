// @review [ ]
use syn::{Attribute, Expr, Meta};

use crate::traits::{Validate, extractor::Extractor};

use super::super::ExtractionState;

// TODO(#attribute/helpers):M[Tr(AttributePattern) => ID(syntax/traits)], "SUPERSEDED by the
//   syntax stage: there is no AttributePattern helper layer. The 'few fixed shapes' are exactly
//   syn::Meta's three variants, so they become three traits (FromPath/FromMetaList/FromNameValue)
//   that a grammar type implements, and #[shape(..)] selects among them at each use site. Hand-
//   matching Meta per extraction type disappears because no extraction type matches Meta any more"
// TODO(#attribute/list):M[F(parse_list) => ID(syntax/traits)], "SUPERSEDED. The mapping was
//   right - a list maps to an enum - but it is not a parse function. It is Tr(FromMetaList)
//   implemented by the enum, with the variants resolved by suffix match against the Node table
//   rather than by comparing idents. Arity comes from the FIELD type (Vec<T> vs T), not from the
//   shape, so a list does not imply a collection"
// TODO(#attribute/path):M[F(parse_path) => ID(syntax/traits)], "SUPERSEDED, and one detail was
//   wrong: #[attr::marker] as an attribute HEAD cannot resolve - VERIFIED, heads resolve in the
//   macro namespace where variants do not exist. The ZST mapping itself stands as Tr(FromPath),
//   and because a ZST value carries the same information as its key, such a field may be written
//   as either (NoClean == no_clean)"
// TODO(#attribute/name-value):M[F(parse_name_value) => ID(syntax/traits)], "SUPERSEDED by
//   Tr(FromNameValue). The struct/tuple mapping stands, with no mixing (#syntax/positional).
//   The rhs needs no parser of ours at all: syn::MetaNameValue::value is already an Expr, which
//   is half of why Expr is the leaf grammar. TransformationExtraction stops being a bespoke
//   Meta matcher and becomes a grammar parse parameterised by the author's type"
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

    //TODO[~](#attribute/validate-body):U[F(validate)], "Empty body - this is one of the 5 live
    //   compile errors. Do not fill it in as written: under the syntax stage validate returns a
    //   narrowed Valid, and the failure side becomes a Reason recorded on the node rather than a
    //   per-type error struct. Blocked on #syntax/extraction and #syntax/reason"
    fn validate(input: &'ast Attribute) -> Result<Self::Valid, Self::ValidityError> {}
}
