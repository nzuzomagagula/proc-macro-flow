// @review [ ]
//! What a stage DECLARES about itself: the node it reads, and the arguments beside it.
//!
//! NOTE(#stage/one-reader-two-generators): V[S(StageDeclaration).used_by(Validate, Processor)],
//! "Attr(derive(Validate)) and Attr(derive(Processor)) read the SAME declaration - `#[source(Ty)]`,
//! plus `#[args(Ty)]` for the one that cares - and differ only in the impl they write. So they
//! share an extractor and a processor and part company at the generator, which is two pipelines
//! over one reader rather than two readers saying the same thing twice.
//!
//! That is the same permission ID(pipeline/no-processor-is-the-extractor) grants in the other
//! direction: a stage type is named by whichever roles it can honestly fill, and nothing says a
//! type may fill a role in only one pipeline."

use proc_macro_flow_traits::assert::Assert;
use proc_macro_flow_traits::extractor::{
    Extracted, Extraction, Extractor, Reason, ReasonKind, Validate,
};
use proc_macro_flow_traits::processor::Processor;
use proc_macro_flow_traits::render::Diagnose;
use syn::{DeriveInput, Generics, Lifetime, Type};

use super::ext::DeriveInputExt;
use super::stage_lifetime;

/// A stage's declaration, as written.
pub(crate) struct StageDeclaration<'ast> {
    pub(crate) name: &'ast syn::Ident,
    pub(crate) declared: &'ast Generics,
    /// `#[source(Ty)]` - the syn node this stage reads.
    pub(crate) source: Type,
    /// `#[args(Ty)]` - present only for an attribute macro's stage.
    pub(crate) args: Option<Type>,
}

impl<'ast> Validate<'ast> for StageDeclaration<'ast> {
    type Source = &'ast DeriveInput;
    /// Nothing to narrow: any item may declare a stage, and what it must DECLARE is read rather
    /// than checked here. ID(pipeline/validity-scope) keeps this surface-level on purpose.
    type Valid = &'ast DeriveInput;

    fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
        Ok(input)
    }
}

impl<'ast> Extractor<'ast> for StageDeclaration<'ast> {
    type Output = Extracted<Self, &'ast DeriveInput>;

    fn extract_from(node: &'ast DeriveInput) -> Self::Output {
        fn read(node: &DeriveInput) -> Extraction<StageDeclaration<'_>> {
            let mut out: Extraction<StageDeclaration<'_>> = Extraction::default();

            // Both reads are attempted whatever the other did - ID(no-result). A stage missing its
            // source AND naming a malformed args type should hear about both.
            let source = match node.source_type() {
                Ok(source) => Some(source),
                Err(error) => {
                    out.reasons.push(Reason::new(ReasonKind::Syntax(error)));
                    None
                }
            };

            let args = match node.args_type() {
                Ok(args) => args,
                Err(error) => {
                    out.reasons.push(Reason::new(ReasonKind::Syntax(error)));
                    None
                }
            };

            if let Some(source) = source {
                out.value = Some(StageDeclaration {
                    name: &node.ident,
                    declared: &node.generics,
                    source,
                    args,
                });
            }

            out
        }

        Extracted::new(read(node), node)
    }
}

impl Assert for StageDeclaration<'_> {}

impl Diagnose for StageDeclaration<'_> {
    /// A leaf: a declaration has no child extractions, only the two types it names.
    fn diagnose(&self, _: &mut Vec<syn::Error>) {}
}

/// A declaration with the impl's generics worked out.
pub(crate) struct ProcessedStage<'ast> {
    pub(crate) name: &'ast syn::Ident,
    /// What to write after `impl` - the type's own generics, plus `'ast` when it declared none.
    pub(crate) generics: Generics,
    /// What to write after the type's name, which is always what the AUTHOR declared.
    pub(crate) declared: &'ast Generics,
    pub(crate) lifetime: Lifetime,
    pub(crate) source: Type,
    pub(crate) args: Option<Type>,
}

impl<'ast> Processor<'ast> for StageDeclaration<'ast> {
    type Input = Extracted<StageDeclaration<'ast>, &'ast DeriveInput>;
    type Output = ProcessedStage<'ast>;

    /// The one real decision these two derives make.
    ///
    /// NOTE(#stage/lifetime-is-the-processing): V[F(process).computes(lifetime)], "It looks like an
    /// identity and is not quite: every stage trait carries `'ast`, but not every stage TYPE
    /// declares one, so the impl's generics are the type's own plus an INTRODUCED lifetime when it
    /// has none (ID(derive/lifetime-is-introduced-when-absent)). Deciding that needs the whole
    /// declaration at once, which is what makes it processing rather than extraction."
    fn process(input: Self::Input) -> Extraction<Self::Output> {
        let node = *input.source();
        // The input's reasons stay where they are - the walk has already rendered them.
        // NOTE(#processor/reasons-are-new-not-inherited).
        let mut out: Extraction<Self::Output> = Extraction::default();

        let Some(value) = input.into_extraction().value else {
            return out;
        };

        let (generics, lifetime) = stage_lifetime(node);

        out.value = Some(ProcessedStage {
            name: value.name,
            generics,
            declared: value.declared,
            lifetime,
            source: value.source,
            args: value.args,
        });
        out
    }
}
