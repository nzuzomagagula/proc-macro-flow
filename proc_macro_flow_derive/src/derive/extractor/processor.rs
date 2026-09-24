// @review [ ]
//! Turning each declared field into the call that fills it.

use proc_macro_flow_traits::extractor::{Extracted, Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::processor::Processor;
use quote::quote;
use syn::{parse2, DeriveInput, Expr};

use super::super::stage::{ProcessedStage, StageDeclaration};
use super::super::{Arity, Child};
use super::extractor::{ExtractorExtraction, Reach};

/// Every field turned into the call that fills it.
pub(crate) struct ProcessedExtractor<'ast> {
    pub(crate) stage: ProcessedStage<'ast>,
    pub(crate) children: Vec<ProcessedChild<'ast>>,
}

/// One field, and the expression that produces its value.
pub(crate) struct ProcessedChild<'ast> {
    pub(crate) ident: &'ast syn::Ident,
    /// A typed Ty(Expr), not a token stream: we BUILT this, so nothing about it is deferred and
    /// ID(typed-output/not-the-carriers) does not apply.
    pub(crate) call: Expr,
}

impl<'ast> Processor<'ast> for ExtractorExtraction<'ast> {
    type Input = Extracted<ExtractorExtraction<'ast>, &'ast DeriveInput>;
    type Output = ProcessedExtractor<'ast>;

    /// The real work: arity off each field's TYPE picks the method, and the reach expression is
    /// spliced into it.
    ///
    /// NOTE(#extractor-derive/arity-picks-the-method): V[F(process).reads(Ty(field))], "`T`,
    /// `Vec<T>` and `Option<T>` select F(extract_from), F(extract_each) and F(extract_maybe), and
    /// the choice is read off the WRITTEN TYPE rather than from anything declared beside it -
    /// ID(from/arity-from-type). These are provided methods on Tr(Extractor), so the call names the
    /// extractor and needs no turbofish (ID(pipeline/no-free-functions))."
    fn process(input: Self::Input) -> Extraction<Self::Output> {
        // NOTE(#processor/reasons-are-new-not-inherited): nothing already in the tree is copied
        // forward, here or in the per-field loop below.
        let mut out: Extraction<Self::Output> = Extraction::default();

        let Some(value) = input.into_extraction().value else {
            return out;
        };

        let Some(stage) = out.absorb(StageDeclaration::process(value.declaration)) else {
            return out;
        };

        let mut children = Vec::new();
        for field in value.fields {
            let Some(declared) = field.into_extraction().value else {
                continue;
            };

            let reach = match &declared.reach {
                Reach::From(expr) => quote!(#expr),
                Reach::With(expr) => quote!((#expr)(source)),
            };

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
            let call = match child.arity {
                Arity::One => quote!( <#extractor>::extract_from(#reach) ),
                Arity::Many => quote!( <#extractor>::extract_each(#reach) ),
                Arity::Maybe => quote!( <#extractor>::extract_maybe(#reach) ),
            };

            match parse2::<Expr>(call) {
                Ok(call) => children.push(ProcessedChild {
                    ident: declared.ident,
                    call,
                }),
                Err(error) => out.reasons.push(Reason::new(ReasonKind::Internal(error))),
            }
        }

        out.value = Some(ProcessedExtractor { stage, children });
        out
    }
}
