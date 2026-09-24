// @review [ ]
//! `#[derive(Extractor)]` — generate `extract_from` from `#[source(..)]` and `#[from]` / `#[with]`.
//!
//! NOTE(#derive/diagnose-rides-along): V[F(generate).emits(Impl(Diagnose))], "Tr(Diagnose) is
//! emitted HERE rather than as a fourth derive, and that does not contradict
//! ID(derive/three-not-one). Those three are separate because each has a real hand-written case:
//! Validate narrows, Processor does work. Tr(Diagnose) has none - it answers only 'where are my
//! children', and for a struct whose every field is a declared child the field list IS the answer,
//! with no alternative an author could want instead. A derive that always emits the same correct
//! thing should not be opt-in; making it one would just be a way to forget it, and a forgotten
//! Tr(Diagnose) is a silently unreachable subtree rather than a compile error"
//!
//! NOTE(#extractor-derive/declaration-is-a-child): V[S(ExtractorExtraction).P(declaration)], "The
//! `#[source(Ty)]` reading is a CHILD EXTRACTION rather than a field read inline, which is what
//! lets its reasons reach the output through the ordinary walk instead of being threaded by hand.
//! It is also what lets this derive share a reader with the two trivial ones
//! (NOTE(#stage/one-reader-two-generators)) rather than reading the same attribute a third time."

use proc_macro_flow_traits::assert::Assert;
use proc_macro_flow_traits::extractor::{
    Extracted, Extraction, Extractor, Reason, ReasonKind, Validate,
};
use proc_macro_flow_traits::generator::Generator;
use proc_macro_flow_traits::pipeline::Pipeline;
use proc_macro_flow_traits::processor::Processor;
use proc_macro_flow_traits::render::Diagnose;
use quote::{quote, ToTokens};
use syn::{parse2, Data, DeriveInput, Expr, Field, Fields, FieldsNamed, ItemImpl};

use super::ext::{AttributeExt, AttributesExt, FieldExt};
use super::stage::{ProcessedStage, StageDeclaration};
use super::{Arity, Child};

/// A whole extraction type, as declared.
pub(crate) struct ExtractorExtraction<'ast> {
    /// `#[source(Ty)]` and the generics, read by the shared reader.
    declaration: Extracted<StageDeclaration<'ast>, &'ast DeriveInput>,
    /// One child per field. Every field of an extraction type IS a child by construction.
    fields: Vec<Extracted<ChildDeclaration<'ast>, &'ast Field>>,
}

/// One field's declaration: what it holds, and where it comes from.
pub(crate) struct ChildDeclaration<'ast> {
    ident: &'ast syn::Ident,
    ty: &'ast syn::Type,
    /// CARRIED, never interpreted - ID(no-parse). A bad expression is rustc's error at the
    /// author's own span once it is spliced.
    reach: Reach,
}

/// The two ways a field says where its children are.
enum Reach {
    /// `#[from(expr)]` - an expression evaluated with `source` in scope.
    From(Expr),
    /// `#[with(callable)]` - applied to `source`.
    With(Expr),
}

impl<'ast> Validate<'ast> for ChildDeclaration<'ast> {
    type Source = &'ast Field;
    type Valid = &'ast Field;

    fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
        Ok(input)
    }
}

impl<'ast> Extractor<'ast> for ChildDeclaration<'ast> {
    type Output = Extracted<Self, &'ast Field>;

    fn extract_from(node: &'ast Field) -> Self::Output {
        fn read(node: &Field) -> Extraction<ChildDeclaration<'_>> {
            let ident = match node.named_ident() {
                Ok(ident) => ident,
                Err(error) => return Extraction::failed(Reason::new(ReasonKind::Syntax(error))),
            };

            let (from, with) = match (node.attrs.find_one("from"), node.attrs.find_one("with")) {
                (Ok(from), Ok(with)) => (from, with),
                (Err(error), _) | (_, Err(error)) => {
                    return Extraction::failed(Reason::new(ReasonKind::Syntax(error)));
                }
            };

            let reach = match (from, with) {
                (Some(_), Some(other)) => {
                    return Extraction::failed(Reason::at(
                        ReasonKind::Ambiguous,
                        other,
                    ));
                }
                (Some(from), None) => from.expr_arg().map(Reach::From),
                (None, Some(with)) => with.expr_arg().map(Reach::With),
                (None, None) => {
                    return Extraction::failed(Reason::at(ReasonKind::Missing, ident));
                }
            };

            match reach {
                Ok(reach) => Extraction::value(ChildDeclaration {
                    ident,
                    ty: &node.ty,
                    reach,
                }),
                Err(error) => Extraction::failed(Reason::new(ReasonKind::Syntax(error))),
            }
        }

        Extracted::new(read(node), node)
    }
}

impl Assert for ChildDeclaration<'_> {}

impl Diagnose for ChildDeclaration<'_> {
    /// A leaf: its `reach` is carried tokens, not a child extraction.
    fn diagnose(&self, _: &mut Vec<syn::Error>) {}
}

impl<'ast> Validate<'ast> for ExtractorExtraction<'ast> {
    type Source = &'ast DeriveInput;
    /// A REAL narrowing, which is what this trait is for: an extraction type is a struct of named
    /// fields, and nothing else can be one.
    type Valid = &'ast FieldsNamed;

    fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
        let Data::Struct(data) = &input.data else {
            return Err(Reason::at(ReasonKind::WrongShape, &input.ident));
        };

        match &data.fields {
            Fields::Named(named) => Ok(named),
            // A tuple struct is all-positional, which cannot say where each child comes from.
            _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
        }
    }
}

impl<'ast> Extractor<'ast> for ExtractorExtraction<'ast> {
    type Output = Extracted<Self, &'ast DeriveInput>;

    fn extract_from(node: &'ast DeriveInput) -> Self::Output {
        let extraction = match Self::validate(node) {
            Ok(named) => Extraction::value(Self {
                declaration: StageDeclaration::extract_from(node),
                fields: ChildDeclaration::extract_each(named.named.iter()),
            }),
            Err(reason) => Extraction::failed(reason),
        };

        Extracted::new(extraction, node)
    }
}

impl Assert for ExtractorExtraction<'_> {}

impl Diagnose for ExtractorExtraction<'_> {
    fn diagnose(&self, out: &mut Vec<syn::Error>) {
        self.declaration.diagnose(out);
        self.fields.diagnose(out);
    }
}

/// Every field turned into the call that fills it.
pub(crate) struct ProcessedExtractor<'ast> {
    stage: ProcessedStage<'ast>,
    children: Vec<ProcessedChild<'ast>>,
}

/// One field, and the expression that produces its value.
pub(crate) struct ProcessedChild<'ast> {
    ident: &'ast syn::Ident,
    /// A typed Ty(Expr), not a token stream: we BUILT this, so nothing about it is deferred and
    /// ID(typed-output/not-the-carriers) does not apply.
    call: Expr,
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

/// Three impls, and the type says three.
pub(crate) struct ExtractorExpansion(ItemImpl, ItemImpl, ItemImpl);

impl ToTokens for ExtractorExpansion {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
        self.1.to_tokens(tokens);
        self.2.to_tokens(tokens);
    }
}

impl<'ast> Generator<'ast> for ExtractorExpansion {
    type Input = ProcessedExtractor<'ast>;
    type Subject = &'ast DeriveInput;
    type Output = Self;

    fn generate(input: ProcessedExtractor<'ast>) -> Extraction<Self> {
        let stage = &input.stage;
        let (impl_generics, _, where_clause) = stage.generics.split_for_impl();
        let (_, type_generics, _) = stage.declared.split_for_impl();
        let (name, lifetime, source) = (stage.name, &stage.lifetime, &stage.source);

        let assignments = input.children.iter().map(|child| {
            let (ident, call) = (child.ident, &child.call);
            quote!(#ident: #call)
        });
        // Every field is a child by construction - reaching it is the whole reason it is declared.
        let visits = input.children.iter().map(|child| {
            let ident = child.ident;
            quote!(::proc_macro_flow_traits::render::Diagnose::diagnose(&self.#ident, out);)
        });

        let node_type = match &stage.args {
            None => quote!(& #lifetime #source),
            Some(args) => quote! {
                ::proc_macro_flow_traits::attributed::Attributed<#lifetime, #args, #source>
            },
        };

        // Parsed SEPARATELY: `parse2::<ItemImpl>` consumes its whole input, so two impls in one
        // call is an error rather than two items. See NOTE(#derive/expansion-is-typed-items).
        let extractor = parse2::<ItemImpl>(quote! {
            impl #impl_generics ::proc_macro_flow_traits::extractor::Extractor<#lifetime>
                for #name #type_generics #where_clause
            {
                type Output = ::proc_macro_flow_traits::extractor::Extracted<
                    Self,
                    #node_type,
                >;

                fn extract_from(node: #node_type) -> Self::Output {
                    // Anonymous imports: the methods below are trait methods, and generated code
                    // must never depend on what happens to be in scope at the call site.
                    use ::proc_macro_flow_traits::extractor::Extractor as _;
                    use ::proc_macro_flow_traits::extractor::Validate as _;

                    let extraction = match <Self as ::proc_macro_flow_traits::extractor::Validate<
                        #lifetime,
                    >>::validate(node)
                    {
                        // `source` names the VALIDATED value, not the raw node - so a narrowing
                        // validate is what every `#[from]` in this struct sees.
                        Ok(source) => ::proc_macro_flow_traits::extractor::Extraction::value(
                            Self { #(#assignments),* },
                        ),
                        Err(reason) => {
                            ::proc_macro_flow_traits::extractor::Extraction::failed(reason)
                        }
                    };

                    // The source rides on the OUTPUT, so it survives the Err arm where there is
                    // no Self to ask.
                    ::proc_macro_flow_traits::extractor::Extracted::new(extraction, node)
                }
            }
        });

        // Where the children are, and nothing else - the `Extracted` around each one renders its
        // reasons, because it holds the node they span against (ID(render/who-renders)).
        let diagnose = parse2::<ItemImpl>(quote! {
            impl #impl_generics ::proc_macro_flow_traits::render::Diagnose
                for #name #type_generics #where_clause
            {
                fn diagnose(
                    &self,
                    out: &mut ::std::vec::Vec<::proc_macro_flow_traits::syn::Error>,
                ) {
                    #(#visits)*
                }
            }
        });

        // Rides along for the same reason Tr(Diagnose) does. Empty, not a descent: every field of
        // an extraction type is an S(Extracted), and a rule inside one is reached by the diagnose
        // walk instead - NOTE(#assert/extracted-is-the-handoff).
        let assert = parse2::<ItemImpl>(quote! {
            impl #impl_generics ::proc_macro_flow_traits::assert::Assert
                for #name #type_generics #where_clause
            {
            }
        });

        match (extractor, diagnose, assert) {
            (Ok(extractor), Ok(diagnose), Ok(assert)) => {
                Extraction::value(ExtractorExpansion(extractor, diagnose, assert))
            }
            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                Extraction::failed(Reason::new(ReasonKind::Internal(error)))
            }
        }
    }

    /// No vacant form - NOTE(#derive/the-impl-is-the-product).
    fn stub(subject: &'ast DeriveInput) -> syn::Result<Self> {
        Err(syn::Error::new_spanned(
            &subject.ident,
            "`#[derive(Extractor)]` needs `#[source(Ty)]` and a `#[from(..)]` or `#[with(..)]` on \
             every field before it can write an impl",
        ))
    }
}

/// `#[derive(Extractor)]`, wired.
// TODO[ ](#extractor/derive-is-a-pipeline): R[F(derive_extractor) -> S(ExtractorWiring)], "The
// first with a REAL validate - a DeriveInput narrowed to named fields - and a real processor:
// arity off each field's written type picks F(extract_from), F(extract_each) or F(extract_maybe)"
pub(crate) struct ExtractorWiring;

impl<'ast> Pipeline<'ast> for ExtractorWiring {
    type Extractor = ExtractorExtraction<'ast>;
    type Processor = ExtractorExtraction<'ast>;
    type Generator = ExtractorExpansion;
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    fn expand(source: &str) -> String {
        let input: DeriveInput = parse_str(source).expect("the item parses");
        ExtractorWiring::run(&input).to_token_stream().to_string()
    }

    const ONE_CHILD: &str = "#[source(DeriveInput)] struct Read<'ast> { \
         #[from(source.attrs.iter())] attrs: Vec<Extracted<Child<'ast>, &'ast Attribute>> }";

    #[test]
    fn three_impls_come_out_and_the_type_says_three() {
        let out = expand(ONE_CHILD);

        assert!(out.contains("Extractor < 'ast > for Read"), "{out}");
        assert!(out.contains("Diagnose for Read"), "{out}");
        assert!(out.contains("Assert for Read"), "{out}");
        assert!(!out.contains("compile_error"), "{out}");
    }

    #[test]
    fn arity_off_the_type_picks_the_method() {
        // ID(extractor-derive/arity-picks-the-method). Nothing beside the field declares this.
        assert!(expand(ONE_CHILD).contains("extract_each"), "Vec should cascade");

        let one = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             #[from(source.ident)] it: Extracted<Child<'ast>, &'ast Ident> }",
        );
        assert!(one.contains("extract_from ("), "{one}");

        let maybe = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             #[from(source.first())] it: Option<Extracted<Child<'ast>, &'ast Ident>> }",
        );
        assert!(maybe.contains("extract_maybe"), "{maybe}");
    }

    #[test]
    fn with_applies_its_callable_to_the_validated_source() {
        let out = expand(
            "#[source(Attribute)] struct Read<'ast> { \
             #[with(|a: &'ast Attribute| ::core::iter::once(a))] \
             own: Vec<Extracted<Child<'ast>, &'ast Attribute>> }",
        );

        assert!(out.contains(") (source)"), "{out}");
    }

    #[test]
    fn diagnose_visits_every_declared_child() {
        // A forgotten visit is a silently unreachable subtree, which is why this is generated
        // rather than opt-in - NOTE(#derive/diagnose-rides-along).
        let out = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             #[from(a())] a: Vec<Extracted<C<'ast>, &'ast I>>, \
             #[from(b())] b: Vec<Extracted<C<'ast>, &'ast I>> }",
        );

        assert!(out.contains("diagnose (& self . a"), "{out}");
        assert!(out.contains("diagnose (& self . b"), "{out}");
    }

    #[test]
    fn a_field_declaring_both_ways_to_reach_it_is_reported() {
        let out = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             #[from(a())] #[with(b)] a: Vec<Extracted<C<'ast>, &'ast I>> }",
        );

        assert!(out.contains("compile_error"), "{out}");
    }

    #[test]
    fn a_field_declaring_neither_is_reported() {
        let out = expand(
            "#[source(DeriveInput)] struct Read<'ast> { a: Vec<Extracted<C<'ast>, &'ast I>> }",
        );

        assert!(out.contains("compile_error"), "{out}");
    }

    #[test]
    fn every_bad_field_is_reported_not_just_the_first() {
        // ID(no-result) through the conversion: the processor carries on past a field it could
        // not read, so a struct with two mistakes reports two.
        let out = expand(
            "#[source(DeriveInput)] struct Read<'ast> { \
             a: Vec<Extracted<C<'ast>, &'ast I>>, \
             b: Vec<Extracted<C<'ast>, &'ast I>> }",
        );

        assert_eq!(out.matches("compile_error").count(), 2, "{out}");
    }

    #[test]
    fn an_enum_is_not_an_extraction_type() {
        // A REAL validate, which is what distinguishes this derive from the two trivial ones.
        let out = expand("#[source(DeriveInput)] enum Read<'ast> { A(&'ast ()) }");

        assert!(out.contains("compile_error"), "{out}");
        assert!(!out.contains("impl < 'ast > :: proc_macro_flow_traits :: extractor"), "{out}");
    }

    #[test]
    fn a_field_that_does_not_hold_an_extraction_is_reported_against_its_type() {
        let out = expand("#[source(DeriveInput)] struct Read<'ast> { #[from(x())] a: &'ast str }");

        assert!(out.contains("compile_error"), "{out}");
        assert!(out.contains("Extracted"), "the message must name the shape wanted: {out}");
    }
}
