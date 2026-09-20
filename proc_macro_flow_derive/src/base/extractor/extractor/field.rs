// @review [ ]
use proc_macro_flow_traits::extractor::{Extracted, Extraction, Extractor, Reason, Validate};
use proc_macro_flow_traits::render::Diagnose;
use proc_macro_flow_traits::resolution::Raw;
use syn::{Attribute, Field};

use crate::base::syntax::extractor::SyntaxFieldAttributeExtraction;

// DEPRECATED(#attribute/generic-grammar):D[S(TransformationExtraction)], "Deleted, and the
// annotations that went with it were wrong in a way worth not rediscovering. They called it 'parse
// this attribute against grammar type G' - a bespoke Meta matcher waiting on the syntax stage. It
// is not that. It held the AUTHOR'S TRANSFORMATION EXPRESSION: how to reach a value from their
// source, as a field path, a closure or a function pointer. That is Attr(from), it belongs on the
// field it describes, and a separate extraction node for it was a category error. The `expression:
// &'ast Expr` placeholder was the tell - it could hold a value but could not say what produced it,
// because nothing was supposed to produce it here at all"
// TODO[x](#field/children):C[S(FieldExtraction).P], "DONE: a field's children are its ATTRIBUTES.
// Two corrections to what this asked for. It said 'empty until the DERIVE gives it fields declared
// with Attr(from)' - which could never happen, because FieldExtraction lives in the proc-macro
// crate and ID(derive/cannot-self-host) means it can never carry the derive where it sits. So the
// stated unblocking condition was impossible, not merely pending. And the real question was never
// which mechanism fills the struct, but WHAT A FIELD'S CHILDREN ARE.
//
// They are the helper attributes written on it, each its own extraction. That is also what first
// puts the syntax stage on the macro path - see Fix[x](#syntax/not-driven)"
pub(crate) struct FieldExtraction<'ast> {
    /// One child per attribute written on the field, grammar or not.
    ///
    /// Attributes that are not ours extract to an empty child carrying no reasons, which is
    /// ID(heads-are-rustcs) - a doc comment is an attribute, and every documented field would
    /// otherwise be an error. They are KEPT rather than filtered because extraction carries and
    /// does not judge; an empty child costs one `Vec` slot and renders nothing.
    pub(crate) attrs: Vec<Extracted<SyntaxFieldAttributeExtraction<'ast, Raw>, &'ast Attribute>>,
}

impl<'ast> Extractor<'ast> for FieldExtraction<'ast> {
    type Output = Extracted<Self, &'ast Field>;

    fn extract_from(node: &'ast Field) -> Self::Output {
        // `validate` is infallible today, so this cannot currently produce a Reason of its own.
        let extraction = match Self::validate(node) {
            Ok(field) => Extraction::value(Self {
                // The children keep their OWN extractions, reasons included - the parent does not
                // absorb them. A reason belongs where it was recorded, and the render walk
                // collects it on the way down (ID(no-ancestry)).
                attrs: SyntaxFieldAttributeExtraction::<Raw>::extract_each(field.attrs.iter()),
            }),
            Err(reason) => Extraction::failed(reason),
        };

        Extracted::new(extraction, node)
    }
}

impl<'ast> Validate<'ast> for FieldExtraction<'ast> {
    type Source = &'ast Field;
    type Valid = &'ast Field;

    // NOTE(#extractor/field-validate): V[F(validate).accepts(all)], "Validates nothing - every
    // Field is accepted, and that is the ANSWER rather than an outstanding task, which is why this
    // is no longer a TODO. Kept honest rather than made to look busy: what there is to check here
    // is whether the field's ATTRIBUTES form a well-shaped grammar node, and each of those is now
    // its own child extraction (ID(field/children)) with its own validate. A check here would
    // duplicate theirs and have a worse span to report it against"
    fn validate(input: &'ast Field) -> Result<Self::Valid, Reason> {
        Ok(input)
    }
}

impl<'ast> Diagnose for FieldExtraction<'ast> {
    /// Where the children are, and nothing else - each `Extracted` renders its own node's reasons,
    /// because it holds the node they span against (NOTE(#render/who-renders)).
    fn diagnose(&self, out: &mut Vec<syn::Error>) {
        self.attrs.diagnose(out);
    }
}
