// @review [ ]
//! Working out the impl's generics. Nothing else needs deriving.

use proc_macro_flow_traits::extractor::{Extracted, Extraction};
use proc_macro_flow_traits::processor::Processor;
use syn::{DeriveInput, Generics};

use super::extractor::WalkDeclaration;

/// The declaration, with the generics the impls are written against.
pub(crate) struct ProcessedWalk<'ast> {
    pub(crate) name: &'ast syn::Ident,
    pub(crate) generics: &'ast Generics,
    pub(crate) fields: Vec<(&'ast syn::Ident, bool)>,
}

impl<'ast> Processor<'ast> for WalkDeclaration<'ast> {
    type Input = Extracted<WalkDeclaration<'ast>, &'ast DeriveInput>;
    type Output = ProcessedWalk<'ast>;

    fn process(input: Self::Input) -> Extraction<Self::Output> {
        // ID(processor/reasons-are-new-not-inherited).
        let mut out: Extraction<Self::Output> = Extraction::default();

        let Some(value) = input.into_extraction().value else {
            return out;
        };

        out.value = Some(ProcessedWalk {
            name: value.name,
            // The type's OWN generics, unchanged: Tr(Diagnose) carries no lifetime of its own, so
            // unlike the stage traits there is nothing to introduce
            // (contrast ID(derive/lifetime-is-introduced-when-absent)).
            generics: value.declared,
            fields: value.fields,
        });
        out
    }
}
