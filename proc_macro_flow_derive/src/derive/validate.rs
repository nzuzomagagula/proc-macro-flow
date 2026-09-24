// @review [ ]
//! `#[derive(Validate)]` — the trivial pass-through, as a pipeline.
//!
//! Separate from `#[derive(Extractor)]` because a real narrowing validate is the interesting case:
//! `StructExtraction` turns a `DeriveInput` into a `&DataStruct`, and a derive that always emitted
//! a trivial one would be unusable for exactly the type that motivated the design. Derive this when
//! there is nothing to check; write it by hand when there is.
//!
//! NOTE(#derive/is-a-pipeline): V[Impl(Pipeline).for(ValidateWiring)], "The derive that generates
//! pipelines is now written as one. What that buys is not symmetry: F(run) owns the normalisation
//! every entry point used to repeat - walk for reasons before processing consumes the tree, choose
//! generate-or-stub, lower, append one compile_error per reason - so the entry function is a single
//! line and the stages hold only what is specific to them (ID(pipeline/owns-normalisation))."

use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use proc_macro_flow_traits::pipeline::Pipeline;
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::{parse2, DeriveInput, Item, ItemImpl, Type};

use super::stage::{ProcessedStage, StageDeclaration};

/// The impl, and the assertions that check what the author NAMED.
///
/// Three fields rather than a `Vec<Item>`: the impl is always written, the source is always
/// asserted, and the args assertion is there exactly when `#[args(Ty)]` was. The type states that,
/// and ID(generation/newtype-per-item) is why it can.
pub(crate) struct ValidateExpansion(ItemImpl, Assertion, Option<Assertion>);

/// One `const _: () = { .. }` proving a named type is what the pipeline will require of it.
pub(crate) struct Assertion(Item);

impl ToTokens for ValidateExpansion {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
        self.1.to_tokens(tokens);
        self.2.to_tokens(tokens);
    }
}

impl ToTokens for Assertion {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
    }
}

impl<'ast> Generator<'ast> for ValidateExpansion {
    type Input = ProcessedStage<'ast>;
    type Subject = &'ast DeriveInput;
    type Output = Self;

    fn generate(input: ProcessedStage<'ast>) -> Extraction<Self> {
        let (impl_generics, _, where_clause) = input.generics.split_for_impl();
        let (_, type_generics, _) = input.declared.split_for_impl();
        let (name, lifetime, source) = (input.name, &input.lifetime, &input.source);

        // NOTE(#validate/source-shape-follows-args): V[Ty(Source) == S(Attributed) <=> Attr(args)],
        // "One input or two, and nothing else changes. A derive keeps the bare borrowed node it
        // always had; an attribute macro gets S(Attributed), which carries BOTH halves of what
        // rustc handed it. Ty(Valid) narrows to the ITEM either way, because that is what a stage
        // downstream wants to look at - the arguments were already read into a grammar by then."
        let source_type: syn::Result<Type> = match &input.args {
            None => parse2(quote!(& #lifetime #source)),
            Some(args) => parse2(quote! {
                ::proc_macro_flow_traits::attributed::Attributed<#lifetime, #args, #source>
            }),
        };

        let narrow = match &input.args {
            None => quote!(::std::result::Result::Ok(input)),
            Some(_) => quote! {
                ::std::result::Result::Ok(
                    ::proc_macro_flow_traits::attributed::Attributed::item(input),
                )
            },
        };

        let item = source_type.and_then(|source_type| {
            parse2::<ItemImpl>(quote! {
                impl #impl_generics ::proc_macro_flow_traits::extractor::Validate<#lifetime>
                    for #name #type_generics #where_clause
                {
                    // Attr(source(Ty)) maps ONE-FOR-ONE onto the associated type - see
                    // NOTE(#pipeline/source-is-associated).
                    type Source = #source_type;
                    type Valid = & #lifetime #source;

                    /// Narrows nothing, and is honest about it: surface-level validation is what
                    /// this trait is for, and this type has no surface check to make.
                    fn validate(
                        input: Self::Source,
                    ) -> ::std::result::Result<
                        Self::Valid,
                        ::proc_macro_flow_traits::extractor::Reason,
                    > {
                        #narrow
                    }
                }
            })
        });

        let mut out: Extraction<Self> = Extraction::default();

        let item = match item {
            Ok(item) => item,
            Err(error) => return Extraction::failed(Reason::new(ReasonKind::Internal(error))),
        };

        let source_assertion = match assert_visitable(&input) {
            Ok(assertion) => assertion,
            Err(error) => return Extraction::failed(Reason::new(ReasonKind::Internal(error))),
        };

        let args_assertion = match input.args.as_ref().map(|args| assert_grammar(&input, args)) {
            None => None,
            Some(Ok(assertion)) => Some(assertion),
            Some(Err(error)) => {
                out.reasons
                    .push(Reason::new(ReasonKind::Internal(error)));
                None
            }
        };

        out.value = Some(ValidateExpansion(item, source_assertion, args_assertion));
        out
    }

    /// NOTE(#derive/the-impl-is-the-product): V[F(stub).R(Err)], "There is no vacant form here, and
    /// saying so is more honest than inventing one. Everywhere else a stub prevents a cascade: a
    /// missing generated impl becomes 'does not implement' at every use site, so a shaped-but-empty
    /// one is strictly better than nothing (ID(generator/stub-is-not-empty)).
    ///
    /// A derive that FAILED TO READ ITS DECLARATION cannot write a shaped one - it does not know
    /// the source type, which is the whole content of the impl. A guessed one would compile and be
    /// wrong, sending the author to debug correct code. So this returns Err, F(run) emits the
    /// reasons alone, and the author gets the one error that names what to fix."
    fn stub(subject: &'ast DeriveInput) -> syn::Result<Self> {
        Err(syn::Error::new_spanned(
            &subject.ident,
            "`#[derive(Validate)]` needs `#[source(Ty)]` before it can write an impl",
        ))
    }
}

/// What `source` must actually BE, checked where the author wrote it.
///
/// NOTE(#declaration/assert-what-was-named): V[F(assert_visitable).at(author_span)], "Attr(source)
/// had been written in every pipeline test since the macro existed and checked by NOTHING, so
/// naming a type that is not a syn node produced either a confusing error deep in generated code or
/// (worse) nothing at all, because the declaration was never read. These are the same
/// `const _ { const fn assert_x<T: Bound>() {} }` assertions ID(shape/bound-at-last) already uses
/// for Attr(shape), spanned with quote_spanned! so the error lands on the type the author named
/// rather than on the item or on code they did not write.
///
/// NOTE(#assert-item/needs-the-generics): V[S(const).wraps(F(generic))], "The check cannot sit
/// directly in a `const _: () = { .. }`, because a const item HAS NO GENERICS and the source type
/// is written against the stage lifetime - `&'ast Field` there is
/// `error[E0261]: use of undeclared lifetime name`. Wrapping it in a function that carries the
/// type's own generics puts the lifetime back in scope. The function is never called; its BODY is
/// what rustc type-checks, and that is where the bound is proved."
fn assert_visitable(input: &ProcessedStage<'_>) -> syn::Result<Assertion> {
    let (impl_generics, _, where_clause) = input.generics.split_for_impl();
    let (lifetime, source) = (&input.lifetime, &input.source);

    parse2::<Item>(quote_spanned! { source.span() =>
        const _: () = {
            // Never called, so rustc would warn about it IN THE AUTHOR'S CRATE - a warning about a
            // line they did not write and cannot silence. Same obligation as the field-init
            // shorthand in `derive/syntax.rs`: generated code owes the same cleanliness as
            // written code.
            #[allow(dead_code)]
            fn assert #impl_generics () #where_clause {
                const fn visitable<
                    #lifetime,
                    T: ::proc_macro_flow_traits::visitable::Visitable<#lifetime>,
                >() {
                }
                visitable::<#lifetime, & #lifetime #source>();
            }
        };
    })
    .map(Assertion)
}

/// And what `args` must be: readable by the ordinary grammar reader.
fn assert_grammar(input: &ProcessedStage<'_>, args: &Type) -> syn::Result<Assertion> {
    let (impl_generics, _, where_clause) = input.generics.split_for_impl();

    parse2::<Item>(quote_spanned! { args.span() =>
        const _: () = {
            #[allow(dead_code)]
            fn assert #impl_generics () #where_clause {
                const fn grammar<T: ::proc_macro_flow_traits::vocab::leaves::FromBody>() {}
                grammar::<#args>();
            }
        };
    })
    .map(Assertion)
}

/// `#[derive(Validate)]`, wired.
pub(crate) struct ValidateWiring;

impl<'ast> Pipeline<'ast> for ValidateWiring {
    type Extractor = StageDeclaration<'ast>;
    /// The extractor names itself, which ID(pipeline/no-processor-is-the-extractor) permits - and
    /// what it does here is real, if small: see NOTE(#stage/lifetime-is-the-processing).
    type Processor = StageDeclaration<'ast>;
    type Generator = ValidateExpansion;
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    /// Drive the WHOLE pipeline, exactly as the entry function does.
    ///
    /// Not the generator alone: what is being checked is that the three stages agree and that
    /// F(run) normalises them, which is the claim the conversion makes.
    fn expand(source: &str) -> String {
        let input: DeriveInput = parse_str(source).expect("the item parses");
        ValidateWiring::run(&input).to_token_stream().to_string()
    }

    #[test]
    fn a_derive_keeps_the_bare_node_as_its_source() {
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("type Source = & 'ast DeriveInput"), "{out}");
        assert!(!out.contains("Attributed"), "a derive got the pair: {out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn declaring_args_makes_the_source_a_pair() {
        // ID(validate/source-shape-follows-args). The whole attribute-macro fix from the derive's
        // side: one extra declaration, and the Source carries both halves.
        let out = expand("#[source(ItemFn)] #[args(TraceArgs)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("Attributed < 'ast , TraceArgs , ItemFn >"), "{out}");
        // And `Valid` still narrows to the ITEM - a stage downstream wants the node, not the pair.
        assert!(out.contains("type Valid = & 'ast ItemFn"), "{out}");
    }

    #[test]
    fn a_type_with_no_lifetime_gets_one_introduced() {
        // ID(derive/lifetime-is-introduced-when-absent), and the one real decision this pipeline's
        // processor makes - NOTE(#stage/lifetime-is-the-processing).
        let out = expand("#[source(DeriveInput)] struct Read;");

        assert!(out.contains("impl < 'ast > :: proc_macro_flow_traits"), "{out}");
        // The type's own name carries NO generics, because the author declared none.
        assert!(out.contains("for Read "), "{out}");
    }

    #[test]
    fn the_source_is_asserted_visitable_at_the_authors_span() {
        // ID(declaration/assert-what-was-named). VERIFIED by probe that the assertion FIRES and
        // lands where it should: `#[source(String)]` gives
        // `the trait bound &'ast String: Visitable<'ast> is not satisfied` pointing at `String` in
        // the attribute, with the syn nodes listed as what does implement it. What this pins is
        // that the assertion is still EMITTED - the half a refactor drops.
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("Visitable"), "the source assertion is gone: {out}");
        assert!(out.contains("const _"), "{out}");
    }

    #[test]
    fn the_args_are_asserted_readable_at_the_authors_span() {
        // VERIFIED by probe: `#[args(String)]` gives `the trait bound String: FromBody is not
        // satisfied` pointing at `String`.
        let out = expand("#[source(ItemFn)] #[args(TraceArgs)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("FromBody"), "the args assertion is gone: {out}");
    }

    #[test]
    fn no_args_means_no_args_assertion() {
        // The absence is the derive case, not an omission - NOTE(#args/absence-is-the-derive-case).
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { _p: &'ast () }");

        assert!(!out.contains("FromBody"), "{out}");
    }

    #[test]
    fn the_generated_assertion_does_not_warn_in_the_authors_crate() {
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("allow (dead_code)"), "{out}");
    }

    #[test]
    fn a_missing_source_is_reported_and_no_impl_is_guessed() {
        // NOTE(#derive/the-impl-is-the-product). The source type IS the content of the impl, so
        // there is nothing to stub - a guessed one would compile and be wrong.
        let out = expand("struct Read<'ast> { _p: &'ast () }");

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("source"), "the message must name what is missing: {out}");
        assert!(!out.contains("impl < 'ast > :: proc_macro_flow_traits"), "an impl was guessed: {out}");
    }

    #[test]
    fn every_reason_reaches_the_output_not_just_the_first() {
        // ID(no-result) through the conversion: the declaration reader attempts BOTH reads
        // whatever the other did.
        let out = expand("#[args(!!)] struct Read<'ast> { _p: &'ast () }");

        assert!(out.matches("compile_error").count() >= 2, "only one reason surfaced: {out}");
    }
}
