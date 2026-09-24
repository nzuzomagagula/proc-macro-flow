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
use syn::{Ident, ItemMod, LitStr, Token, Type};

use crate::derive::ext::TypeExt;

use super::extractor::{ComponentExtraction, Emission, PipelineExtraction, PipelineSource, Role};
use super::generator::PipelineArgs;

/// A pipeline, resolved: three stages that exist, and the helpers rustc must be told about.
pub(crate) struct ProcessedPipeline<'ast> {
    pub(crate) extractor: &'ast Ident,
    pub(crate) processor: &'ast Ident,
    pub(crate) generator: &'ast Ident,
    /// Lifted from the vocabulary named by `helpers = ..`, never redeclared.
    pub(crate) helpers: Vec<LitStr>,

    /// The node the extractor reads, from `source = Ty` on the extractor role.
    ///
    /// NOTE(#pipeline-macro/source-was-never-read): V[S(ProcessedPipeline).P(source)], "`source`
    /// has been written in every pipeline test since the macro existed and read by NOTHING - the
    /// processor looked only at `from` and `helpers`. It went unnoticed because the entry function
    /// hardcoded Ty(DeriveInput) for a derive, which is right for a derive and silently wrong for
    /// everything else. Reading it is what lets an entry parse the node its pipeline actually
    /// declared, and it is resolution over the whole module, which is this stage's job"
    pub(crate) source: Type,

    /// The grammar the attribute's own arguments are read into, from `args = Ty`.
    ///
    /// `None` is a derive - see NOTE(#args/absence-is-the-derive-case).
    pub(crate) args: Option<Type>,

    /// What the generator does to the item it was applied to, from `emits = ..`.
    pub(crate) emission: Emission,

    /// The module itself, so the generator can re-emit it stripped.
    ///
    /// NOTE(#pipeline-macro/processed-carries-the-node): V[S(ProcessedPipeline).P(module)],
    /// "S(PipelineInput) used to exist to carry these two alongside the processed value, because
    /// the generator needed them and Tr(Processor)::Output could not reach them. It can: the
    /// processor is handed the whole S(Extracted), source included, so it can put on its output
    /// whatever the next stage needs. That is ID(processor/receives-whole) being spent rather than
    /// merely stated, and S(PipelineInput) collapses into this."
    pub(crate) module: &'ast ItemMod,

    /// What `#[pipeline(..)]` ITSELF was invoked with.
    ///
    /// Named apart from `args` above, which is the grammar an authored attribute macro reads ITS
    /// arguments into. Two different attributes' arguments meet in this struct and calling both
    /// `args` would be the kind of collision that compiles.
    pub(crate) written: &'ast PipelineArgs,
}

/// `key = Value` pairs inside a role attribute.
struct Args(Vec<(Ident, Type)>);

impl Parse for Args {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let pairs = Punctuated::<Arg, Token![,]>::parse_terminated(input)?;
        Ok(Args(pairs.into_iter().map(|arg| (arg.key, arg.value)).collect()))
    }
}

struct Arg {
    key: Ident,
    value: Type,
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key = input.parse()?;
        input.parse::<Token![=]>()?;
        // NOTE(#pipeline-macro/values-are-types): V[S(Arg).P(value).T(Type)], "Was Ty(Ident),
        // which was right while the only values were `from = Struct` and `helpers = Vocabulary` -
        // both of which name a SIBLING in this module, and a sibling is a bare name. `source` and
        // `args` are not siblings: they name TYPES, and a type may be qualified
        // (`proc_macro2::TokenStream`) or generic. So the value widens to Ty(Type) and the two
        // sibling-naming keys narrow back with F(as_ident), which answers None for anything that
        // could not be a sibling in the first place"
        let value = input.parse()?;
        Ok(Arg { key, value })
    }
}

impl Args {
    fn get(&self, key: &str) -> Option<&Type> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// The value of `key`, when it names something a sibling in this module could be called.
    fn sibling(&self, key: &str) -> Option<&Ident> {
        self.get(key).and_then(TypeExt::as_ident)
    }
}

// TODO[ ](#pipeline/declarations-are-read): U[S(ProcessedPipeline).P(source)] && C[E(Emission)],
// "`source = Ty` was written in every pipeline test since the macro existed and read by NOTHING -
// the entry hardcoded Ty(DeriveInput), which is right for a derive and silently wrong for
// everything else. Resolving it needs the whole module at once, which is this stage's job"
impl<'ast> Processor<'ast> for PipelineExtraction<'ast> {
    type Input = Extracted<PipelineExtraction<'ast>, PipelineSource<'ast>>;
    type Output = ProcessedPipeline<'ast>;

    fn process(input: Self::Input) -> Extraction<Self::Output> {
        let node = *input.source();
        let (module, written) = (node.item(), node.args());
        let extraction = input.into_extraction();

        // NOTE(#processor/reasons-are-new-not-inherited): the extraction's own reasons are the
        // walk's to render, not this stage's to repeat.
        let mut out: Extraction<ProcessedPipeline<'ast>> = Extraction::default();

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
                match from.as_ident() {
                    Some(name) if present.contains(&name) => {}
                    // Either it names nothing here, or it is not the shape a sibling's name can
                    // take at all (`a::B`, `B<T>`). Both are the same complaint to the author:
                    // this does not name a component in this module.
                    _ => out.reasons.push(Reason::at(ReasonKind::UnknownKey, from)),
                }
            }
        }

        // `helpers = Y` must name a vocabulary declared in this module, whose spellings we lift.
        let helpers = match components
            .iter()
            .find_map(|(_, args)| args.sibling("helpers"))
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

        // `source = Ty` on the EXTRACTOR, which is the role that declares what it reads. Required:
        // without it an entry function has no node to parse - see
        // NOTE(#pipeline-macro/source-was-never-read).
        let declared = |role: Role, key: &str| -> Option<Type> {
            components
                .iter()
                .find(|(component, _)| component.roles.contains(&role))
                .and_then(|(_, args)| args.get(key))
                .cloned()
        };

        let source = declared(Role::Extractor, "source");
        let declared_args = declared(Role::Extractor, "args");

        if source.is_none() {
            out.reasons
                .push(Reason::at(ReasonKind::Missing, &module.ident));
        }

        // `emits = ..` on the GENERATOR - NOTE(#pipeline-macro/emission-must-be-declared).
        let emission = match declared(Role::Generator, "emits") {
            None => Emission::default(),
            Some(written) => match written.as_ident().and_then(|name| {
                Emission::from_spelling(&name.to_string())
            }) {
                Some(emission) => emission,
                None => {
                    out.reasons
                        .push(Reason::at(ReasonKind::UnknownKey, &written));
                    Emission::default()
                }
            },
        };

        if let (Some(extractor), Some(processor), Some(generator), Some(source)) =
            (extractor, processor, generator, source)
        {
            out.value = Some(ProcessedPipeline {
                extractor,
                processor,
                generator,
                helpers,
                source,
                args: declared_args,
                emission,
                module,
                written,
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
        let args: &'static PipelineArgs = Box::leak(Box::new(PipelineArgs {
            kind: crate::base::pipeline::generator::MacroKind::Derive,
            exported: parse_str("Thing").expect("an ident"),
            entry: true,
        }));
        PipelineExtraction::process(PipelineExtraction::extract_from(
            proc_macro_flow_traits::attributed::Attributed::new(args, item),
        ))
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
    #[test]
    fn the_source_is_read_off_the_extractor() {
        // NOTE(#pipeline-macro/source-was-never-read). This declaration had been written in every
        // test since the macro existed and read by nothing.
        let out = processed(
            r#"mod m {
                #[extractor(source = ItemFn)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P)] struct G;
            }"#,
        );
        let value = out.value.expect("resolves");

        assert_eq!(
            quote::ToTokens::to_token_stream(&value.source).to_string(),
            "ItemFn"
        );
        assert!(value.args.is_none(), "a derive has no arguments");
    }

    #[test]
    fn a_qualified_source_survives_being_a_type() {
        // ID(pipeline-macro/values-are-types). While a value was an Ident this did not parse at
        // all, which is why a function-like pipeline could not name its own source.
        let out = processed(
            r#"mod m {
                #[extractor(source = proc_macro2::TokenStream)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P)] struct G;
            }"#,
        );
        let value = out.value.expect("resolves");

        assert_eq!(
            quote::ToTokens::to_token_stream(&value.source).to_string(),
            "proc_macro2 :: TokenStream"
        );
    }

    #[test]
    fn declaring_args_is_what_makes_it_an_attribute_macro() {
        let out = processed(
            r#"mod m {
                #[extractor(source = ItemFn, args = TraceArgs)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P)] struct G;
            }"#,
        );
        let value = out.value.expect("resolves");

        let args = value.args.expect("the arguments were declared");
        assert_eq!(quote::ToTokens::to_token_stream(&args).to_string(), "TraceArgs");
    }

    #[test]
    fn a_pipeline_with_no_source_is_reported() {
        let out = processed(
            r#"mod m {
                #[extractor(helpers = H)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P)] struct G;
            }"#,
        );

        assert!(out.value.is_none(), "an entry has no node to parse");
        assert!(!out.reasons.is_empty());
    }

    #[test]
    fn emission_defaults_to_adding_beside_the_item() {
        // The safe default: a generator that declares nothing cannot silently DELETE the item.
        let out = processed(
            r#"mod m {
                #[extractor(source = ItemFn)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P)] struct G;
            }"#,
        );

        assert_eq!(out.value.expect("resolves").emission, Emission::Beside);
    }

    #[test]
    fn a_generator_may_declare_that_it_rewrites_the_item() {
        let out = processed(
            r#"mod m {
                #[extractor(source = ItemFn)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P, emits = replace)] struct G;
            }"#,
        );

        assert_eq!(out.value.expect("resolves").emission, Emission::Replace);
    }

    #[test]
    fn an_emission_we_do_not_have_is_reported_and_falls_back_safely() {
        let out = processed(
            r#"mod m {
                #[extractor(source = ItemFn)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P, emits = obliterate)] struct G;
            }"#,
        );

        // Reported - and the fallback is the one that cannot lose the author's code.
        assert!(!out.reasons.is_empty(), "an unknown emission went unreported");
        assert_eq!(out.value.expect("resolves").emission, Emission::Beside);
    }

}
