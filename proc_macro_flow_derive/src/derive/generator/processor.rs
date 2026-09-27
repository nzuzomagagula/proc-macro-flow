// @review [ ]
//! Turning each declared child into the statements that produce it, and its vacant form.

use proc_macro_flow_traits::extractor::{Extracted, Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::processor::Processor;
use syn::{DeriveInput, Ident, Stmt, Type};

use super::extractor::GeneratorDeclaration;

/// Every declared child turned into the statements that produce it.
pub(crate) struct ProcessedGenerator<'ast> {
    pub(crate) name: &'ast Ident,
    pub(crate) declared: &'ast syn::Generics,
    pub(crate) generics: syn::Generics,
    pub(crate) lifetime: syn::Lifetime,
    pub(crate) from: Type,
    pub(crate) subject: Type,
    pub(crate) names: Vec<Ident>,
    pub(crate) calls: Vec<Stmt>,
    pub(crate) stubs: Vec<Stmt>,
    pub(crate) indices: Vec<syn::Index>,
}

impl<'ast> Processor<'ast> for GeneratorDeclaration<'ast> {
    type Input = Extracted<GeneratorDeclaration<'ast>, &'ast DeriveInput>;
    type Output = ProcessedGenerator<'ast>;

    /// The real work: each declared child becomes a statement that calls it, absorbs its result
    /// and isolates its failure, and a second that produces its vacant form.
    fn process(input: Self::Input) -> Extraction<Self::Output> {
        let node = *input.source();
        // ID(processor/reasons-are-new-not-inherited).
        let mut out: Extraction<Self::Output> = Extraction::default();

        let Some(value) = input.into_extraction().value else {
            return out;
        };

        // A leaf wrapping a syn item borrows nothing, so it may have no lifetime of its own - see
        // ID(derive/lifetime-is-introduced-when-absent).
        let (generics, lifetime) = super::super::stage::stage_lifetime(node);

        let mut names = Vec::new();
        let mut calls = Vec::new();
        let mut stubs = Vec::new();
        for child in &value.children {
            match (child.call(), child.stub()) {
                (Ok(call), Ok(stub)) => {
                    names.push(child.name.clone());
                    calls.push(call);
                    stubs.push(stub);
                }
                (Err(error), _) | (_, Err(error)) => {
                    out.reasons.push(Reason::new(ReasonKind::Internal(error)));
                }
            }
        }

        out.value = Some(ProcessedGenerator {
            name: value.name,
            declared: value.declared,
            generics,
            lifetime,
            from: value.wiring.from,
            subject: value.wiring.subject,
            names,
            calls,
            stubs,
            indices: value.indices,
        });
        out
    }
}
