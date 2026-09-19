// @review [ ]
use proc_macro_flow_traits::extractor::{Extracted, Extraction, Extractor, Validate};
use syn::Field;

// DEPRECATED(#attribute/generic-grammar):D[S(TransformationExtraction)], "Deleted, and the
// annotations that went with it were wrong in a way worth not rediscovering. They called it 'parse
// this attribute against grammar type G' - a bespoke Meta matcher waiting on the syntax stage. It
// is not that. It held the AUTHOR'S TRANSFORMATION EXPRESSION: how to reach a value from their
// source, as a field path, a closure or a function pointer. That is Attr(from), it belongs on the
// field it describes, and a separate extraction node for it was a category error. The `expression:
// &'ast Expr` placeholder was the tell - it could hold a value but could not say what produced it,
// because nothing was supposed to produce it here at all"
pub(crate) struct FieldExtraction<'ast> {
    // TODO[ ](#field/children):C[S(FieldExtraction).P], "Empty until the derive gives it fields
    // declared with Attr(from). What used to sit here - a Vec of TransformationExtraction - was the
    // transformation EXPRESSION mistaken for a child node"
    _marker: ::core::marker::PhantomData<&'ast ()>,
}

pub struct FieldExtractionError;

impl<'ast> Extractor<'ast, &'ast Field> for FieldExtraction<'ast> {
    type Output = Extracted<Self, &'ast Field>;

    fn extract_from(node: &'ast Field) -> Self::Output {
        // `validate` is infallible today, so this cannot currently produce a Reason of its own.
        let extraction = match Self::validate(node) {
            Ok(_) => Extraction::value(Self {
                _marker: ::core::marker::PhantomData,
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
