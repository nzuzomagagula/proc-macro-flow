// @review [ ]
use proc_macro_flow_traits::extractor::{Extracted, Extraction, Extractor, Reason, Validate};
use proc_macro_flow_traits::render::Diagnose;
use proc_macro_flow_traits::resolution::Raw;
use syn::{Attribute, Field};

use crate::base::syntax::extractor::SyntaxFieldAttributeExtraction;

// NOTE(#attribute/generic-grammar): a field's Attr(from) expression is NOT a child node. It says how
// to reach a value, which belongs on the field, not in an extraction of its own.
// NOTE(#field/children): a field's children are its ATTRIBUTES, each its own extraction. This is what
// puts the syntax stage on the macro path.
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
