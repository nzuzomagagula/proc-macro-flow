// @review [ ]
//! Reading a type that wants its walk written: the fields, and which of them are skipped.

use proc_macro_flow_traits::assert::Assert;
use proc_macro_flow_traits::extractor::{
    Extracted, Extraction, Extractor, Reason, ReasonKind, Validate,
};
use proc_macro_flow_traits::render::Diagnose;
use syn::{Data, DeriveInput, Fields, FieldsNamed};

use super::super::ext::{AttributesExt, FieldExt};

/// A type whose `Diagnose` is to be written for it.
pub(crate) struct WalkDeclaration<'ast> {
    pub(crate) name: &'ast syn::Ident,
    pub(crate) declared: &'ast syn::Generics,
    /// Every field, and whether it is descended into.
    pub(crate) fields: Vec<(&'ast syn::Ident, bool)>,
}

impl<'ast> Validate<'ast> for WalkDeclaration<'ast> {
    type Source = &'ast DeriveInput;
    type Valid = &'ast FieldsNamed;

    fn validate(input: Self::Source) -> ::std::result::Result<Self::Valid, Reason> {
        let Data::Struct(data) = &input.data else {
            return Err(Reason::at(ReasonKind::WrongShape, &input.ident));
        };

        match &data.fields {
            Fields::Named(named) => Ok(named),
            _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
        }
    }
}

impl<'ast> Extractor<'ast> for WalkDeclaration<'ast> {
    type Output = Extracted<Self, &'ast DeriveInput>;

    fn extract_from(node: &'ast DeriveInput) -> Self::Output {
        fn read(node: &DeriveInput) -> Extraction<WalkDeclaration<'_>> {
            let named = match WalkDeclaration::validate(node) {
                Ok(named) => named,
                Err(reason) => return Extraction::failed(reason),
            };

            let mut out: Extraction<WalkDeclaration<'_>> = Extraction::default();
            let mut fields = Vec::new();

            for field in &named.named {
                let ident = match field.named_ident() {
                    Ok(ident) => ident,
                    Err(error) => {
                        out.reasons.push(Reason::new(ReasonKind::Syntax(error)));
                        continue;
                    }
                };

                match field.attrs.find_one("skip") {
                    Ok(skip) => fields.push((ident, skip.is_none())),
                    Err(error) => out.reasons.push(Reason::new(ReasonKind::Syntax(error))),
                }
            }

            out.value = Some(WalkDeclaration {
                name: &node.ident,
                declared: &node.generics,
                fields,
            });
            out
        }

        Extracted::new(read(node), node)
    }
}

impl Assert for WalkDeclaration<'_> {}

impl Diagnose for WalkDeclaration<'_> {
    /// A leaf: a field list is data, not child extractions.
    fn diagnose(&self, _: &mut Vec<syn::Error>) {}
}
