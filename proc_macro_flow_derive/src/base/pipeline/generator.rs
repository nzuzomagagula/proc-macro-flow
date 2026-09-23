// @review [ ]
//! Emitting a pipeline: the module back, its wiring, and the macro entry point.
//!
//! NOTE(#pipeline-macro/entry-is-a-child): V[S(PipelineExpansion).P(Option<Entry>)], "Whether the
//! entry function is emitted is `Option<Entry>` - a CHILD THAT MAY NOT BE THERE - and not a branch
//! inside a code path. `entry = manual` simply produces None, and `Option<T>: ToTokens` emits
//! nothing for it.
//!
//! So the toggle is the SHAPE OF THE TREE, which is the same rule ID(from/arity-from-type) applies
//! to extraction: what may be absent says so in its type"

use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse2, Ident, ItemFn, ItemImpl, ItemMod, ItemStruct, Token};

use super::processor::ProcessedPipeline;

proc_macro_flow_traits::vocabulary! {
    /// Which kind of proc macro this pipeline becomes.
    ///
    /// Closed, because rustc has exactly these three and they are not ours to extend.
    pub enum MacroKind {
        Derive = "derive",
        Attribute = "attribute",
        Function = "function",
    }
}

/// `#[pipeline(derive = FieldNames, entry = manual)]`.
pub(crate) struct PipelineArgs {
    pub(crate) kind: MacroKind,
    /// The name the macro is exported under.
    pub(crate) exported: Ident,
    /// False for `entry = manual` - the author writes their own entry function.
    pub(crate) entry: bool,
}

impl Parse for PipelineArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut kind = None;
        let mut entry = true;

        for pair in Punctuated::<Assign, Token![,]>::parse_terminated(input)? {
            let key = pair.key.to_string();
            if key == "entry" {
                entry = pair.value != "manual";
                continue;
            }
            match MacroKind::from_spelling(&key) {
                Some(found) => kind = Some((found, pair.value)),
                None => {
                    return Err(syn::Error::new_spanned(
                        &pair.key,
                        format!(
                            "expected one of: {}, `entry` - got `{key}`",
                            MacroKind::candidates()
                        ),
                    ));
                }
            }
        }

        let (kind, exported) = kind.ok_or_else(|| {
            input.error(format!(
                "name the macro kind and its exported name, one of: {}",
                MacroKind::candidates()
            ))
        })?;

        Ok(PipelineArgs {
            kind,
            exported,
            entry,
        })
    }
}

struct Assign {
    key: Ident,
    value: Ident,
}

impl Parse for Assign {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(Assign {
            key,
            value: input.parse()?,
        })
    }
}

/// Everything a pipeline expansion needs, gathered by the entry function.
///
/// NOTE(#pipeline/subject-equals-source-breaks-attributes): V[Tr(Pipeline).Ty(Subject) == Ty(Source)],
/// "The pipeline macro does NOT use Tr(Pipeline)::run, and the reason is a real limitation worth
/// recording rather than bootstrapping convenience. Tr(Pipeline) binds
/// `Generator::Subject = Validate::Source`, so a generator can only be handed the same node the
/// extractor read. An ATTRIBUTE macro has two inputs - the attribute's arguments and the item - and
/// the arguments are not part of the item, so there is no Source that carries both.
///
/// Ty(PipelineInput) is that pair, assembled by the entry function. The general fix is
/// F(run_attribute) taking both, which is what ID(pipeline/macro-kind) is for; until then this one
/// macro drives its own stages"
pub(crate) struct PipelineInput<'ast> {
    pub(crate) processed: ProcessedPipeline<'ast>,
    pub(crate) args: &'ast PipelineArgs,
    pub(crate) module: &'ast ItemMod,
}

/// The module, its wiring, and - when asked for - the entry point.
pub(crate) struct PipelineExpansion(ItemMod, Wiring, Option<Entry>);

/// `struct X; impl Pipeline<'ast> for X { .. }` - two items, so two fields.
pub(crate) struct Wiring(ItemStruct, ItemImpl);

/// `#[proc_macro_*] pub fn ..`
pub(crate) struct Entry(ItemFn);

impl ToTokens for PipelineExpansion {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
        self.1.to_tokens(tokens);
        self.2.to_tokens(tokens);
    }
}

impl ToTokens for Wiring {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
        self.1.to_tokens(tokens);
    }
}

impl ToTokens for Entry {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
    }
}

/// The marker type the `Pipeline` impl hangs on.
fn wiring_name(exported: &Ident) -> Ident {
    quote::format_ident!("{}Wiring", exported)
}

impl<'ast> Generator<'ast> for Wiring {
    type Input = &'ast PipelineInput<'ast>;
    type Subject = &'ast Ident;
    type Output = Self;

    fn generate(input: &'ast PipelineInput<'ast>) -> Extraction<Self> {
        let name = wiring_name(&input.args.exported);
        let (extractor, processor, generator) = (
            input.processed.extractor,
            input.processed.processor,
            input.processed.generator,
        );
        let module = &input.module.ident;

        let marker = parse2::<ItemStruct>(quote! {
            #[doc = "Wiring generated by `#[pipeline]`."]
            pub struct #name;
        });

        let wiring = parse2::<ItemImpl>(quote! {
            impl<'ast> ::proc_macro_flow_traits::pipeline::Pipeline<'ast> for #name {
                type Extractor = #module::#extractor<'ast>;
                type Processor = #module::#processor<'ast>;
                type Generator = #module::#generator<'ast>;
            }
        });

        match (marker, wiring) {
            (Ok(marker), Ok(wiring)) => Extraction::value(Wiring(marker, wiring)),
            (Err(error), _) | (_, Err(error)) => {
                Extraction::failed(Reason::new(ReasonKind::Internal(error)))
            }
        }
    }

    fn stub(exported: &'ast Ident) -> syn::Result<Self> {
        let name = wiring_name(exported);
        Ok(Wiring(
            parse2(quote!(pub struct #name;))?,
            // A vacant impl would not type-check, so the stub is the marker alone. This is the one
            // place NOTE(#generator/stub-is-not-empty) cannot promise the full shape.
            parse2(quote!(impl #name {}))?,
        ))
    }
}

impl<'ast> Generator<'ast> for Entry {
    type Input = &'ast PipelineInput<'ast>;
    type Subject = &'ast Ident;
    type Output = Self;

    fn generate(input: &'ast PipelineInput<'ast>) -> Extraction<Self> {
        let exported = &input.args.exported;
        let wiring = wiring_name(exported);
        let name = quote::format_ident!("{}", heck::ToSnakeCase::to_snake_case(exported.to_string().as_str()));
        // NOTE(#pipeline-macro/helpers-are-idents): V[Attr(proc_macro_derive).attributes(Ident)],
        // "`attributes(..)` takes BARE IDENTS, not string literals - `attributes("shape")` is a
        // parse error at the definition site. The vocabulary stores spellings as Ty(LitStr)
        // because that is what a spelling IS at every other use, so the conversion belongs here,
        // at the one place that needs the other form.
        //
        // It can fail: a vocabulary may spell a key `snake-case` legitimately, and that is not an
        // ident. Failing is a Reason rather than a panic, and it is OURS - the author declared a
        // spelling their grammar can read but a derive helper cannot be named after"
        let mut helpers: Vec<Ident> = Vec::new();
        let mut spelling_failures: Vec<Reason> = Vec::new();
        for spelling in &input.processed.helpers {
            match syn::parse_str::<Ident>(&spelling.value()) {
                Ok(ident) => helpers.push(ident),
                Err(_) => spelling_failures.push(Reason::at(
                    ReasonKind::WrongShape,
                    spelling,
                )),
            }
        }
        let helpers = &helpers;

        // Parse, run, lower ONCE. What differs per kind is the node parsed and whether the item
        // is re-emitted - see NOTE(#pipeline/attribute-re-emits).
        let parse = |target: proc_macro2::TokenStream, call: proc_macro2::TokenStream| {
            quote! {
                let parsed = match ::proc_macro_flow_traits::syn::parse::<#target>(input) {
                    ::std::result::Result::Ok(parsed) => parsed,
                    ::std::result::Result::Err(error) => {
                        return ::proc_macro_flow_traits::quote::ToTokens::to_token_stream(
                            &error.to_compile_error(),
                        )
                        .into();
                    }
                };

                ::proc_macro_flow_traits::quote::ToTokens::to_token_stream(&#call).into()
            }
        };

        let run = quote!(<#wiring as ::proc_macro_flow_traits::pipeline::Pipeline>::run(&parsed));
        let run_attribute =
            quote!(<#wiring as ::proc_macro_flow_traits::pipeline::Pipeline>::run_attribute(&parsed));

        let derive_body = parse(quote!(::proc_macro_flow_traits::syn::DeriveInput), run.clone());
        let attribute_body = parse(quote!(::proc_macro_flow_traits::syn::Item), run_attribute);
        let function_body = parse(quote!(::proc_macro_flow_traits::proc_macro2::TokenStream), run);

        let item = match input.args.kind {
            MacroKind::Derive => parse2::<ItemFn>(quote! {
                #[proc_macro_derive(#exported, attributes(#(#helpers),*))]
                pub fn #name(input: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
                    #derive_body
                }
            }),
            MacroKind::Attribute => parse2::<ItemFn>(quote! {
                #[proc_macro_attribute]
                pub fn #name(
                    _attr: ::proc_macro::TokenStream,
                    input: ::proc_macro::TokenStream,
                ) -> ::proc_macro::TokenStream {
                    #attribute_body
                }
            }),
            // Function-like takes RAW TOKENS: there is no node to narrow, so `Source` is the
            // TokenStream itself - VERIFIED Visitable (visitable.rs:41) - and `Validate` is
            // honestly vacant rather than invented.
            MacroKind::Function => parse2::<ItemFn>(quote! {
                #[proc_macro]
                pub fn #name(input: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
                    #function_body
                }
            }),
        };

        let mut out = match item {
            Ok(item) => Extraction::value(Entry(item)),
            Err(error) => Extraction::failed(Reason::new(ReasonKind::Internal(error))),
        };
        out.reasons.extend(spelling_failures);
        out
    }

    fn stub(exported: &'ast Ident) -> syn::Result<Self> {
        let name = quote::format_ident!("{}", heck::ToSnakeCase::to_snake_case(exported.to_string().as_str()));
        parse2(quote! {
            pub fn #name(input: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
                input
            }
        })
        .map(Entry)
    }
}

impl<'ast> Generator<'ast> for PipelineExpansion {
    type Input = &'ast PipelineInput<'ast>;
    type Subject = &'ast ItemMod;
    type Output = Self;

    fn generate(input: &'ast PipelineInput<'ast>) -> Extraction<Self> {
        let mut out: Extraction<Self> = Extraction::default();
        let exported = &input.args.exported;

        let wiring = match out.absorb(Wiring::generate(input)) {
            Some(wiring) => wiring,
            None => match Wiring::stub(exported) {
                Ok(vacant) => vacant,
                Err(error) => {
                    out.reasons
                        .push(Reason::new(ReasonKind::Internal(error)));
                    return out;
                }
            },
        };

        // The toggle: a child that may not be there - NOTE(#pipeline-macro/entry-is-a-child).
        let entry = match input.args.entry {
            false => None,
            true => match out.absorb(Entry::generate(input)) {
                Some(entry) => Some(entry),
                None => Entry::stub(exported).ok(),
            },
        };

        out.value = Some(PipelineExpansion(
            super::extractor::stripped(input.module),
            wiring,
            entry,
        ));
        out
    }

    fn stub(module: &'ast ItemMod) -> syn::Result<Self> {
        // The module ALONE. Whatever failed, the author's own code still compiles.
        Ok(PipelineExpansion(
            super::extractor::stripped(module),
            Wiring::stub(&module.ident)?,
            None,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::base::pipeline::expand;
    use quote::quote;

    /// One well-formed pipeline module, reused by every kind - what differs is only the ARGUMENT.
    fn module() -> proc_macro2::TokenStream {
        quote! {
            mod stages {
                #[extractor(source = DeriveInput, helpers = Helper)]
                struct Read;

                #[processor(from = Read)]
                struct Understood;

                #[generator(from = Understood)]
                struct Built;

                vocabulary! {
                    pub enum Helper {
                        Shape = "shape",
                        Alias = "alias",
                    }
                }
            }
        }
    }

    fn expanded(attr: proc_macro2::TokenStream) -> String {
        expand(attr, module()).to_string()
    }

    #[test]
    fn a_derive_pipeline_emits_a_proc_macro_derive() {
        let out = expanded(quote!(derive = FieldNames));

        assert!(out.contains("proc_macro_derive"), "{out}");
        assert!(out.contains("pub fn field_names"), "the snake_case entry is missing: {out}");
        assert!(out.contains("DeriveInput"), "a derive parses a DeriveInput: {out}");
        // NOT `!contains("compile_error")` - the generated BODY legitimately calls
        // `error.to_compile_error()` on a parse failure. What must be absent is a compile_error
        // WE emitted, which is the macro form.
        assert!(!out.contains("compile_error !"), "the expansion complained: {out}");
    }

    #[test]
    fn a_derive_pipeline_registers_the_helpers_it_declared() {
        // ID(pipeline/helpers-are-syntactic), closed. The spellings come from the VOCABULARY in the
        // module - nothing restates them - and land in the one place rustc will accept them.
        let out = expanded(quote!(derive = FieldNames));

        // Asserted on the WHOLE attribute, not on the spellings alone: the module is re-emitted
        // with its vocabulary, so `"shape"` appears in the output whether or not the entry ever
        // registered it. That is how this test first passed while the generated code was wrong.
        //
        // ID(pipeline-macro/helpers-are-idents): BARE IDENTS, no quotes. rustc rejects
        // `attributes("shape")` at the definition site.
        assert!(
            out.contains("attributes (shape , alias)"),
            "the helpers were not registered as idents: {out}"
        );
    }

    #[test]
    fn a_spelling_that_is_not_an_ident_is_reported_not_emitted() {
        // The other half of ID(pipeline-macro/helpers-are-idents). A vocabulary may legitimately
        // spell a key in a way no ident can be named after; that is a complaint here, not a
        // panic, and not silently dropped either.
        let module = quote! {
            mod stages {
                #[extractor(source = DeriveInput, helpers = Helper)]
                struct Read;
                #[processor(from = Read)]
                struct Understood;
                #[generator(from = Understood)]
                struct Built;

                vocabulary! {
                    pub enum Helper {
                        Kebab = "not-an-ident",
                    }
                }
            }
        };
        let out = expand(quote!(derive = FieldNames), module).to_string();

        assert!(out.contains("compile_error !"), "the bad spelling went unreported: {out}");
        assert!(
            !out.contains("attributes (not"),
            "a non-ident spelling reached the definition site: {out}"
        );
    }

    #[test]
    fn an_attribute_pipeline_re_emits_the_item() {
        // ID(pipeline/attribute-re-emits). An attribute macro REPLACES what it annotates, so the
        // entry must call run_attribute - calling `run` would silently delete the author's item.
        let out = expanded(quote!(attribute = Trace));

        assert!(out.contains("proc_macro_attribute"), "{out}");
        assert!(out.contains("run_attribute"), "an attribute entry called plain `run`: {out}");
        assert!(!out.contains("proc_macro_derive"), "{out}");
    }

    #[test]
    fn a_function_like_pipeline_parses_raw_tokens() {
        // No node to narrow, so the source IS the TokenStream - VERIFIED Visitable.
        let out = expanded(quote!(function = expand_it));

        assert!(out.contains("# [proc_macro]"), "{out}");
        assert!(out.contains("TokenStream"), "{out}");
        assert!(!out.contains("DeriveInput"), "a function-like macro has no DeriveInput: {out}");
    }

    #[test]
    fn manual_entry_emits_the_wiring_and_no_function() {
        // NOTE(#pipeline-macro/entry-is-a-child): the toggle is a child that is not there, so this
        // is the SAME code path with a `None` in it - not a second branch.
        let out = expanded(quote!(derive = FieldNames, entry = manual));

        assert!(out.contains("struct FieldNamesWiring"), "{out}");
        assert!(out.contains("Pipeline"), "{out}");
        assert!(!out.contains("proc_macro_derive"), "an entry was emitted anyway: {out}");
        assert!(!out.contains("pub fn field_names"), "{out}");
    }

    #[test]
    fn every_kind_wires_the_three_stages_through_the_module() {
        // Whatever the kind, the Pipeline impl names the stages by their path INSIDE the module -
        // the module is re-emitted, so its items are not in scope at the wiring's site.
        for attr in [
            quote!(derive = A),
            quote!(attribute = B),
            quote!(function = C),
        ] {
            let out = expand(attr, module()).to_string();
            assert!(out.contains("stages :: Read"), "{out}");
            assert!(out.contains("stages :: Understood"), "{out}");
            assert!(out.contains("stages :: Built"), "{out}");
        }
    }

    #[test]
    fn a_kind_we_do_not_have_is_named_with_the_ones_we_do() {
        let out = expand(quote!(macro_rules = Nope), module()).to_string();

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("derive"), "the message must list what IS accepted: {out}");
        // The author's module survives the complaint - an attribute macro that emits only an error
        // has deleted their code.
        assert!(out.contains("mod stages"), "{out}");
    }

    #[test]
    fn a_broken_pipeline_still_emits_the_module_and_the_wiring_marker() {
        // NOTE(#generator/stub-is-not-empty) at the pipeline's own level: `from = Nope` resolves to
        // nothing, so there is no impl to write - but deleting the module would turn one bad
        // attribute into an error at every use site.
        let broken = quote! {
            mod stages {
                #[extractor(source = DeriveInput)]
                struct Read;
                #[processor(from = Nope)]
                struct Understood;
                #[generator(from = Understood)]
                struct Built;
            }
        };
        let out = expand(quote!(derive = FieldNames), broken).to_string();

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("mod stages"), "{out}");
        assert!(out.contains("FieldNamesWiring"), "{out}");
    }
}
