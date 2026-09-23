// @review [ ]
//! Narrowing a pipeline module to what generation needs, and checking the graph on the way.
//!
//! NOTE(#pipeline-macro/graph-check-is-processing): V[F(process).resolves(from)], "Whether every
//! `from = X` names something PRESENT is the processor's business and not the extractor's, and the
//! division is the usual one. Extraction walks and carries: it can see that an attribute says
//! `from = Nope`, but not whether `Nope` exists, because it is looking at one item at a time.
//! Processing sees the whole module at once, which is exactly what resolving a name needs.
//!
//! Every failure here is spanned against the ATTRIBUTE that caused it, so an author is pointed at
//! their own declaration rather than at the module"

use proc_macro_flow_traits::extractor::{Extracted, Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::processor::Processor;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, ItemMod, LitStr, Token};

use super::extractor::{ComponentExtraction, PipelineExtraction, Role};

/// A pipeline, resolved: three stages that exist, and the helpers rustc must be told about.
pub(crate) struct ProcessedPipeline<'ast> {
    pub(crate) extractor: &'ast Ident,
    pub(crate) processor: &'ast Ident,
    pub(crate) generator: &'ast Ident,
    /// Lifted from the vocabulary named by `helpers = ..`, never redeclared.
    pub(crate) helpers: Vec<LitStr>,
}

/// `key = Value` pairs inside a role attribute.
struct Args(Vec<(Ident, Ident)>);

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let pairs = Punctuated::<Arg, Token![,]>::parse_terminated(input)?;
        Ok(Args(pairs.into_iter().map(|arg| (arg.key, arg.value)).collect()))
    }
}

struct Arg {
    key: Ident,
    value: Ident,
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key = input.parse()?;
        input.parse::<Token![=]>()?;
        // The value is a bare name - `from = Struct`, `helpers = SyntaxHelper`. A path would be a
        // different feature; a name is what a sibling in this module has.
        let value = input.parse()?;
        Ok(Arg { key, value })
    }
}

impl Args {
    fn get(&self, key: &str) -> Option<&Ident> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }
}

impl<'ast> Processor<'ast> for PipelineExtraction<'ast> {
    type Input = Extracted<PipelineExtraction<'ast>, &'ast ItemMod>;
    type Output = ProcessedPipeline<'ast>;

    fn process(input: Self::Input) -> Extraction<Self::Output> {
        let module = *input.source();
        let extraction = input.into_extraction();

        let mut out: Extraction<ProcessedPipeline<'ast>> = Extraction {
            value: None,
            reasons: extraction.reasons,
        };

        let Some(value) = extraction.value else {
            return out;
        };

        // The components that actually extracted, paired with their parsed arguments.
        let mut components: Vec<(&ComponentExtraction<'ast>, Args)> = Vec::new();
        for child in &value.components {
            let Some(component) = child.value() else {
                continue;
            };
            match syn::parse2::<Args>(component.args.clone()) {
                Ok(args) => components.push((component, args)),
                Err(error) => out.reasons.push(Reason::new(ReasonKind::Syntax(error))),
            }
        }

        // EXACTLY ONE of each role. Two extractors is not a pipeline, and neither is none.
        let named = |role: Role, out: &mut Extraction<ProcessedPipeline<'ast>>| {
            let found: Vec<&(&ComponentExtraction<'ast>, Args)> = components
                .iter()
                .filter(|(component, _)| component.roles.contains(&role))
                .collect();

            match found.as_slice() {
                [(component, _)] => Some(component.name),
                [] => {
                    out.reasons.push(Reason::at(
                        ReasonKind::Missing,
                        &module.ident,
                    ));
                    None
                }
                [_, extra, ..] => {
                    out.reasons.push(Reason::at(ReasonKind::Duplicate, extra.0.name));
                    None
                }
            }
        };

        let extractor = named(Role::Extractor, &mut out);
        let processor = named(Role::Processor, &mut out);
        let generator = named(Role::Generator, &mut out);

        // Every `from = X` must name a component PRESENT in this module.
        let present: Vec<&Ident> = components.iter().map(|(c, _)| c.name).collect();
        for (_, args) in &components {
            if let Some(from) = args.get("from") {
                if !present.iter().any(|name| *name == from) {
                    out.reasons.push(Reason::at(ReasonKind::UnknownKey, from));
                }
            }
        }

        // `helpers = Y` must name a vocabulary declared in this module, whose spellings we lift.
        let helpers = match components
            .iter()
            .find_map(|(_, args)| args.get("helpers"))
        {
            None => Vec::new(),
            Some(named) => {
                match value
                    .vocabularies
                    .iter()
                    .filter_map(|v| v.value())
                    .find(|vocabulary| vocabulary.name == *named)
                {
                    Some(vocabulary) => vocabulary.spellings.clone(),
                    None => {
                        // NAMED BUT ABSENT. Precisely reportable, which is the whole gain over
                        // the hand-synced list - see NOTE(#pipeline-macro/helpers-are-read).
                        out.reasons.push(Reason::at(ReasonKind::UnknownKey, named));
                        Vec::new()
                    }
                }
            }
        };

        if let (Some(extractor), Some(processor), Some(generator)) = (extractor, processor, generator)
        {
            out.value = Some(ProcessedPipeline {
                extractor,
                processor,
                generator,
                helpers,
            });
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro_flow_traits::extractor::Extractor;
    use syn::parse_str;

    fn processed(source: &str) -> Extraction<ProcessedPipeline<'static>> {
        let item: &'static ItemMod = Box::leak(Box::new(
            parse_str(source).expect("the module parses"),
        ));
        PipelineExtraction::process(PipelineExtraction::extract_from(item))
    }

    const WHOLE: &str = r#"
        mod field_names {
            #[extractor(source = DeriveInput, helpers = SyntaxHelper)]
            struct Struct;
            #[processor(from = Struct)]
            struct Processed;
            #[generator(from = Processed)]
            struct Block;
            vocabulary! { pub enum SyntaxHelper { Shape = "shape", Alias = "alias" } }
        }
    "#;

    #[test]
    fn a_whole_pipeline_resolves_to_three_stages_and_its_helpers() {
        let out = processed(WHOLE);
        let value = out.value.expect("the pipeline resolves");

        assert_eq!(value.extractor, "Struct");
        assert_eq!(value.processor, "Processed");
        assert_eq!(value.generator, "Block");

        let helpers: Vec<String> = value.helpers.iter().map(|h| h.value()).collect();
        assert_eq!(helpers, ["shape", "alias"], "lifted from the vocabulary, not redeclared");
    }

    #[test]
    fn a_dangling_from_is_caught() {
        // The graph check - NOTE(#pipeline-macro/graph-check-is-processing). Extraction could see
        // the attribute; only processing can see whether `Nope` exists.
        let out = processed(
            r#"mod m {
                #[extractor(source = DeriveInput)] struct S;
                #[processor(from = Nope)] struct P;
                #[generator(from = P)] struct G;
            }"#,
        );

        assert_eq!(out.reasons.len(), 1);
        assert_eq!(out.reasons[0].message(), "not a key this node accepts");
    }

    #[test]
    fn a_missing_role_is_reported() {
        let out = processed(r#"mod m { #[extractor(source = D)] struct S; }"#);

        // no processor, no generator
        assert_eq!(out.reasons.len(), 2);
        assert!(out.value.is_none(), "a pipeline without three stages is not one");
    }

    #[test]
    fn two_of_a_role_is_reported() {
        let out = processed(
            r#"mod m {
                #[extractor(source = D)] struct A;
                #[extractor(source = D)] struct B;
                #[processor(from = A)] struct P;
                #[generator(from = P)] struct G;
            }"#,
        );

        assert!(out.reasons.iter().any(|r| r.message() == "written more than once"));
    }

    #[test]
    fn a_helper_vocabulary_that_is_not_there_is_reported() {
        // The gain over a hand-synced list: naming something absent is precisely reportable.
        let out = processed(
            r#"mod m {
                #[extractor(source = D, helpers = Missing)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P)] struct G;
            }"#,
        );

        assert_eq!(out.reasons.len(), 1);
        assert_eq!(out.reasons[0].message(), "not a key this node accepts");
    }

    #[test]
    fn a_pipeline_with_no_helpers_is_fine() {
        let out = processed(
            r#"mod m {
                #[extractor(source = D)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P)] struct G;
            }"#,
        );

        assert!(out.reasons.is_empty());
        assert!(out.value.expect("resolves").helpers.is_empty());
    }
}
