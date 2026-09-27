// @review [ ]
//! Turning each declared field into the call that fills it.

use proc_macro_flow_traits::extractor::{Extracted, Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::processor::Processor;
use quote::quote;
use syn::{parse2, DeriveInput, Expr};

use super::super::stage::{ProcessedStage, StageDeclaration};
use proc_macro_flow_traits::node::Arity;

use super::super::ext::Child;
use super::extractor::{ExtractorExtraction, Reach};

/// Every field turned into the call that fills it.
pub(crate) struct ProcessedExtractor<'ast> {
    pub(crate) stage: ProcessedStage<'ast>,
    /// Named `fields` and not `children`: the moment a field may hold a value, "child" is a claim
    /// about some of them and a lie about the rest.
    pub(crate) fields: Vec<ProcessedField<'ast>>,
}

/// One field, and the expression that produces its value.
pub(crate) struct ProcessedField<'ast> {
    pub(crate) ident: &'ast syn::Ident,
    /// A typed Ty(Expr), not a token stream: we BUILT this, so nothing about it is deferred and
    /// ID(typed-output/not-the-carriers) does not apply.
    pub(crate) call: Expr,
    /// Whether the render walk descends into this field.
    ///
    /// False for a `#[value]`, and that is the half of
    /// ID(extractor-derive/children-are-marked-not-inferred) with teeth: a plain `&'ast Ident`
    /// does not implement Tr(Diagnose), so emitting a visit for one is a compile error in the
    /// AUTHOR'S crate rather than a wrong answer here.
    pub(crate) walked: bool,
}

impl<'ast> Processor<'ast> for ExtractorExtraction<'ast> {
    type Input = Extracted<ExtractorExtraction<'ast>, &'ast DeriveInput>;
    type Output = ProcessedExtractor<'ast>;

    /// The real work: arity off each field's TYPE picks the method, and the reach expression is
    /// spliced into it.
    ///
    /// NOTE(#extractor-derive/arity-picks-the-method): V[F(process).has(reads the field type)], "T, Vec and Option pick extract_from, extract_each, extract_maybe"
    /// `T`, `Vec<T>` and `Option<T>` select F(extract_from), F(extract_each) and F(extract_maybe),
    /// and the choice is read off the WRITTEN TYPE rather than from anything declared beside it -
    /// ID(from/arity-from-type). These are provided methods on Tr(Extractor), so the call names the
    /// extractor and needs no turbofish (ID(pipeline/no-free-functions)).
    fn process(input: Self::Input) -> Extraction<Self::Output> {
        // ID(processor/reasons-are-new-not-inherited): nothing already in the tree is copied
        // forward, here or in the per-field loop below.
        let mut out: Extraction<Self::Output> = Extraction::default();

        let Some(value) = input.into_extraction().value else {
            return out;
        };

        let Some(stage) = out.absorb(StageDeclaration::process(value.declaration)) else {
            return out;
        };

        let mut fields = Vec::new();
        for field in value.fields {
            let Some(declared) = field.into_extraction().value else {
                continue;
            };

            let expr = declared.reach.expr();
            let reach = match &declared.reach {
                Reach::From(_) => quote!(#expr),
                Reach::With(_) => quote!((#expr)(source)),
                // A value IS its expression. Nothing is called, nothing is walked, and
                // F(Child::of) is never consulted - so a `#[value]` field may be any type at all.
                Reach::Value(_) => quote!(#expr),
            };

            let call = if declared.reach.is_child() {
                // Only HERE does the type have to be an extraction, and the error for a field
                // that is not one is the one this derive has always given.
                let child = match Child::of(declared.ty) {
                    Ok(child) => child,
                    // `Child::of` spans its own error against the offending TYPE, so the reason
                    // needs no fallback of its own - ID(reason/span-not-node)'s finer pointer is
                    // already inside the carried error.
                    Err(error) => {
                        out.reasons.push(Reason::new(ReasonKind::Syntax(error)));
                        continue;
                    }
                };

                let extractor = &child.extractor;
                match child.arity {
                    Arity::Required => quote!( <#extractor>::extract_from(#reach) ),
                    Arity::Repeated => quote!( <#extractor>::extract_each(#reach) ),
                    Arity::Optional => quote!( <#extractor>::extract_maybe(#reach) ),
                }
            } else {
                reach
            };

            match parse2::<Expr>(call) {
                Ok(call) => fields.push(ProcessedField {
                    ident: declared.ident,
                    call,
                    walked: declared.reach.is_child(),
                }),
                Err(error) => out.reasons.push(Reason::new(ReasonKind::Internal(error))),
            }
        }

        out.value = Some(ProcessedExtractor { stage, fields });
        out
    }
}
