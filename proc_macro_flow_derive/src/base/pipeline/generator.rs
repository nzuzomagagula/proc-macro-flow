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

use super::extractor::{Emission, PipelineSource};
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
///
/// NOTE(#pipeline-args/stays-hand-written): V[!S(PipelineArgs).derive(Syntax)], "The plan had this
/// becoming an ordinary Attr(derive(Syntax)) grammar read through Tr(FromBody), so that
/// Attr(pipeline)'s own surface got the same treatment it gives everyone else. It is NOT, for two
/// reasons found on attempting it, and both are worth recording so the idea is not re-had.
///
/// FIRST, it cannot: ID(derive/cannot-self-host) is verified - `can't use a procedural macro from
/// the same crate that defines it` - and Attr(derive(Syntax)) is defined in this crate. The only
/// route would be hand-writing Tr(FromBody) and Tr(Described), which is writing out what the derive
/// exists to generate.
///
/// SECOND, and this is the part that settles it, it would buy nothing. A grammar node is a FIXED
/// KEY SET with an arity each; these arguments are a CHOICE among three heads - `derive`,
/// `attribute`, `function` - where the head is the datum. Phase B's `one_of` could state that as
/// three Option fields, but the hand-written F(parse) below already enforces exactly one and
/// already produces the candidate list from E(MacroKind), which is the same single source of truth.
///
/// Revisit if Attr(pipeline) ever grows genuine key-value options, where the grammar model would
/// start paying for itself"
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

/// Emits the arguments back as they were written.
///
/// NOTE(#pipeline-args/tokens-for-the-span-only): V[Impl(ToTokens).for(PipelineArgs).span_only],
/// "Exists because Tr(Pipeline)::run bounds `Source: ToTokens` and S(Attributed) emits both its
/// halves, and it is used for ONE thing: the node a reason with nothing finer of its own falls
/// back to (ID(reason/span-not-node)). Nothing lowers these tokens into anybody's crate - the
/// module and the wiring are what the macro emits, and they are built elsewhere entirely.
///
/// It is reconstructed from the fields rather than a stored copy of the input, which keeps
/// ID(no-owned-nodes) intact: nothing here holds tokens the author wrote."
impl ToTokens for PipelineArgs {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        let kind = quote::format_ident!("{}", self.kind.spelling());
        let exported = &self.exported;
        tokens.extend(quote!(#kind = #exported));

        if !self.entry {
            tokens.extend(quote!(, entry = manual));
        }
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

// ID(pipeline/subject-equals-source-breaks-attributes) is CLOSED, and S(PipelineInput) went with
// it. It existed to carry the args and the module beside the processed value, because
// Tr(Pipeline) appeared unable to; see NOTE(#pipeline-macro/is-a-pipeline) for why that reading was
// wrong and NOTE(#pipeline-macro/processed-carries-the-node) for where those two live now.

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

// TODO[x](#pipeline/generated-entry-compiles): V[Attr(proc_macro_derive).generated.compiles],
// "Every proof of Attr(pipeline) so far asserted the entry function as TOKENS, because a
// Attr(proc_macro_derive) is legal only in a `proc-macro = true` crate and neither the derive crate
// (which defines Attr(pipeline)) nor the facade (which is an ordinary lib) can be one. So the
// generated entry had never been handed to rustc, and did not in fact compile: the wiring marker
// was `pub`, which such a crate forbids outright"

/// The marker type the `Pipeline` impl hangs on.
fn wiring_name(exported: &Ident) -> Ident {
    quote::format_ident!("{}Wiring", exported)
}

impl<'ast> Generator<'ast> for Wiring {
    type Input = &'ast ProcessedPipeline<'ast>;
    type Subject = &'ast Ident;
    type Output = Self;

    fn generate(input: &'ast ProcessedPipeline<'ast>) -> Extraction<Self> {
        let name = wiring_name(&input.written.exported);
        let (extractor, processor, generator) = (
            input.extractor,
            input.processor,
            input.generator,
        );
        let module = &input.module.ident;

        // NOTE(#pipeline-macro/wiring-is-private): V[S(Wiring).marker.!pub], "PRIVATE, and it has
        // to be. A generated entry point is only legal in a `proc-macro = true` crate, and rustc
        // refuses one that exports ANY item other than a macro function:
        // `proc-macro crate types currently cannot export any items other than functions tagged
        // with #[proc_macro]...`. A `pub` marker therefore made the generated-entry path
        // uncompilable in the one crate kind that can host it - which went unnoticed because every
        // proof of Attr(pipeline) until now ran under `entry = manual` and asserted TOKENS.
        //
        // Nothing needs it public: the entry function is emitted as its SIBLING, so they are always
        // in the same module. The same constraint is recorded from the other side at
        // ID(syntax/placement)."
        let marker = parse2::<ItemStruct>(quote! {
            #[doc = "Wiring generated by `#[pipeline]`."]
            struct #name;
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
            parse2(quote!(struct #name;))?,
            // A vacant impl would not type-check, so the stub is the marker alone. This is the one
            // place NOTE(#generator/stub-is-not-empty) cannot promise the full shape.
            parse2(quote!(impl #name {}))?,
        ))
    }
}

// TODO[x](#pipeline/entry-reads-args): U[F(Entry::generate).emits(two_input_parse)], "The
// generated attribute entry took `_attr` and threw it away, so `#[trace(level = ..)]` could not be
// written at all. It parses both inputs now, reading the arguments through the ordinary grammar
// reader"
impl<'ast> Generator<'ast> for Entry {
    type Input = &'ast ProcessedPipeline<'ast>;
    type Subject = &'ast Ident;
    type Output = Self;

    fn generate(input: &'ast ProcessedPipeline<'ast>) -> Extraction<Self> {
        let exported = &input.written.exported;
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
        for spelling in &input.helpers {
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

        let source = &input.source;
        let derive_body = parse(quote!(#source), run.clone());
        let function_body = parse(quote!(#source), run.clone());

        // THE TWO-INPUT PARSE. Everything above takes one token stream; an attribute macro takes
        // the arguments AND the item, and until this existed the arguments were discarded outright
        // - the generated signature read `_attr: TokenStream` and `#[trace(level = "debug")]` could
        // not be written at all. See NOTE(#attributed/source-is-a-pair).
        //
        // The arguments go through Tr(FromBody), which is the ORDINARY grammar reader - the same
        // one a helper attribute goes through (NOTE(#from-body/one-reader-two-entries)). `&parsed`
        // is the fallback node, so a bare `#[trace]` missing a required key underlines the item it
        // was written on rather than nothing.
        let attribute_body = match &input.args {
            Some(args) => {
                let call = match input.emission {
                    // The generator adds BESIDE the item, so the framework hands the item back.
                    Emission::Beside => quote! {
                        <#wiring as ::proc_macro_flow_traits::pipeline::Pipeline>::run_attribute(
                            ::proc_macro_flow_traits::attributed::Attributed::new(&arguments, &parsed),
                        )
                    },
                    // The generator REWRITES the item, so re-emitting it here would double it.
                    Emission::Replace => quote! {
                        <#wiring as ::proc_macro_flow_traits::pipeline::Pipeline>::run(
                            ::proc_macro_flow_traits::attributed::Attributed::new(&arguments, &parsed),
                        )
                    },
                };

                quote! {
                    let parsed = match ::proc_macro_flow_traits::syn::parse::<#source>(input) {
                        ::std::result::Result::Ok(parsed) => parsed,
                        ::std::result::Result::Err(error) => {
                            return ::proc_macro_flow_traits::quote::ToTokens::to_token_stream(
                                &error.to_compile_error(),
                            )
                            .into();
                        }
                    };

                    let arguments = match <#args as
                        ::proc_macro_flow_traits::vocab::leaves::FromBody>::from_body(
                            &::std::convert::Into::into(attr),
                            &parsed,
                        )
                    {
                        ::std::result::Result::Ok(arguments) => arguments,
                        // The author's ITEM is not deleted when their arguments are wrong - an
                        // attribute macro replaces what it annotates, so emitting only the error
                        // would take their code with it.
                        ::std::result::Result::Err(error) => {
                            let mut out = ::proc_macro_flow_traits::quote::ToTokens::to_token_stream(&parsed);
                            ::std::iter::Extend::extend(&mut out, error.to_compile_error());
                            return out.into();
                        }
                    };

                    ::proc_macro_flow_traits::quote::ToTokens::to_token_stream(&#call).into()
                }
            }
            // An attribute pipeline that declared no `args` has a bare node for a Source, so
            // Tr(Annotated) is not implemented for it and F(run_attribute) would not compile.
            // Reported HERE, where the author can be told what is missing, rather than as a trait
            // error inside generated code.
            None => {
                let message = format!(
                    "`attribute = {exported}` needs `args = Ty` on its extractor: an attribute \
                     macro is handed its own arguments as well as the item, and without a grammar \
                     to read them into they would be discarded"
                );
                quote! {
                    ::std::compile_error!(#message);
                }
            }
        };

        let item = match input.written.kind {
            MacroKind::Derive => parse2::<ItemFn>(quote! {
                #[proc_macro_derive(#exported, attributes(#(#helpers),*))]
                pub fn #name(input: ::proc_macro::TokenStream) -> ::proc_macro::TokenStream {
                    #derive_body
                }
            }),
            MacroKind::Attribute => parse2::<ItemFn>(quote! {
                #[proc_macro_attribute]
                pub fn #name(
                    attr: ::proc_macro::TokenStream,
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
    /// BY VALUE, because Tr(Pipeline) binds `Generator::Input = Processor::Output` and a processor
    /// hands its output over rather than lending it. S(Wiring) and S(Entry) below still take a
    /// borrow - they are parts of this expansion, not stages of the pipeline, and the borrow of a
    /// local resolves against a shorter Ty(Generator) lifetime by covariance.
    type Input = ProcessedPipeline<'ast>;
    type Subject = PipelineSource<'ast>;
    type Output = Self;

    fn generate(input: ProcessedPipeline<'ast>) -> Extraction<Self> {
        let mut out: Extraction<Self> = Extraction::default();
        let exported = &input.written.exported;
        let module = input.module;

        let wiring = match out.absorb(Wiring::generate(&input)) {
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
        let entry = match input.written.entry {
            false => None,
            true => match out.absorb(Entry::generate(&input)) {
                Some(entry) => Some(entry),
                None => Entry::stub(exported).ok(),
            },
        };

        out.value = Some(PipelineExpansion(
            super::extractor::stripped(module),
            wiring,
            entry,
        ));
        out
    }

    fn stub(node: PipelineSource<'ast>) -> syn::Result<Self> {
        let module = node.item();
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

    /// A module whose EXTRACTOR declaration varies, because that is what now decides the shape of
    /// the entry: `source` says what to parse and `args` says whether there is a second input.
    fn declaring(extractor: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
        quote! {
            mod stages {
                #[extractor(#extractor)]
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
        let module = declaring(quote!(source = ItemFn, args = TraceArgs, helpers = Helper));
        let out = expand(quote!(attribute = Trace), module).to_string();

        assert!(out.contains("proc_macro_attribute"), "{out}");
        assert!(out.contains("run_attribute"), "an attribute entry called plain `run`: {out}");
        assert!(!out.contains("proc_macro_derive"), "{out}");
    }

    #[test]
    fn an_attribute_entry_actually_reads_its_arguments() {
        // THE hole this phase exists to close. The entry used to be generated with
        // `_attr: TokenStream` - the arguments were discarded outright, so `#[trace(level = ..)]`
        // could not be written at all. See NOTE(#attributed/source-is-a-pair).
        let module = declaring(quote!(source = ItemFn, args = TraceArgs, helpers = Helper));
        let out = expand(quote!(attribute = Trace), module).to_string();

        // `_attr :`, not `_attr` - `run_attribute` contains that substring, so the looser check
        // failed against output that was already correct.
        assert!(!out.contains("_attr :"), "the arguments are still discarded: {out}");
        assert!(out.contains("attr : :: proc_macro :: TokenStream"), "{out}");
        // Read by the ORDINARY grammar reader, not a parser of its own.
        assert!(out.contains("FromBody"), "{out}");
        assert!(out.contains("Attributed :: new"), "{out}");
    }

    #[test]
    fn an_attribute_pipeline_without_args_is_told_what_is_missing() {
        // Without `args` the Source is a bare node, so Tr(Annotated) is not implemented and
        // `run_attribute` would fail as a trait error deep inside generated code. Reported here
        // instead, where the author can act on it.
        let module = declaring(quote!(source = ItemFn, helpers = Helper));
        let out = expand(quote!(attribute = Trace), module).to_string();

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("args = Ty"), "the message must say what to write: {out}");
    }

    #[test]
    fn a_generator_that_rewrites_the_item_does_not_re_emit_it() {
        // NOTE(#pipeline-macro/emission-must-be-declared). `replace` means the generator's own
        // output already contains the item, so handing it back as well would double it.
        let module = quote! {
            mod stages {
                #[extractor(source = ItemFn, args = TraceArgs)]
                struct Read;
                #[processor(from = Read)]
                struct Understood;
                #[generator(from = Understood, emits = replace)]
                struct Built;
            }
        };
        let out = expand(quote!(attribute = Trace), module).to_string();

        assert!(!out.contains("run_attribute"), "the item would be emitted twice: {out}");
        assert!(out.contains("Pipeline > :: run ("), "{out}");
    }

    #[test]
    fn a_function_like_pipeline_parses_what_its_source_declares() {
        // No node to narrow, so the source IS the TokenStream - VERIFIED Visitable. What changed
        // is that the entry now parses what `source` DECLARES rather than a hardcoded guess:
        // NOTE(#pipeline-macro/source-was-never-read).
        let module = declaring(quote!(source = proc_macro2::TokenStream, helpers = Helper));
        let out = expand(quote!(function = expand_it), module).to_string();

        assert!(out.contains("# [proc_macro]"), "{out}");
        assert!(out.contains("parse :: < proc_macro2 :: TokenStream >"), "{out}");
        assert!(!out.contains("DeriveInput"), "a function-like macro has no DeriveInput: {out}");
    }

    #[test]
    fn a_derive_parses_the_node_its_source_declares() {
        let module = declaring(quote!(source = ItemStruct, helpers = Helper));
        let out = expand(quote!(derive = Thing), module).to_string();

        assert!(out.contains("parse :: < ItemStruct >"), "a hardcoded node came back: {out}");
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
