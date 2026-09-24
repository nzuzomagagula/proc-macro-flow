// @review [ ]
//! What `#[derive(Validate)]` builds: one impl, and the assertions that check what was named.

use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::{parse2, DeriveInput, Item, ItemImpl, Type};

use super::processor::ProcessedStage;

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

        // NOTE(#validate/source-shape-follows-args): V[Ty(Source) ==? S(Attributed)], "The Source is Attributed exactly when #[args] is written"
        //
        // One input or two, and nothing else changes. A derive keeps the bare borrowed node it
        // always had; an attribute macro gets S(Attributed), which carries BOTH halves of what
        // rustc handed it. Ty(Valid) narrows to the ITEM either way, because that is what a stage
        // downstream wants to look at - the arguments were already read into a grammar by then.
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
                    // ID(pipeline/source-is-associated).
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

    /// NOTE(#derive/the-impl-is-the-product): V[F(stub).R(Err)], "There is no vacant form; the impl is the product"
    /// There is no vacant form here, and saying so is more honest than inventing one. Everywhere
    /// else a stub prevents a cascade: a missing generated impl becomes 'does not implement' at
    /// every use site, so a shaped-but-empty one is strictly better than nothing
    /// (ID(generator/stub-is-not-empty)).
    ///
    /// A derive that FAILED TO READ ITS DECLARATION cannot write a shaped one - it does not know
    /// the source type, which is the whole content of the impl. A guessed one would compile and be
    /// wrong, sending the author to debug correct code. So this returns Err, F(run) emits the
    /// reasons alone, and the author gets the one error that names what to fix.
    fn stub(subject: &'ast DeriveInput) -> syn::Result<Self> {
        Err(syn::Error::new_spanned(
            &subject.ident,
            "`#[derive(Validate)]` needs `#[source(Ty)]` before it can write an impl",
        ))
    }
}

/// What `source` must actually BE, checked where the author wrote it.
///
/// NOTE(#declaration/assert-what-was-named): V[F(assert_visitable).has(author span)], "#[source] is checked to name a syn node, at the author's span"
/// Attr(source) had been written in every pipeline test since the macro existed and checked by
/// NOTHING, so naming a type that is not a syn node produced either a confusing error deep in
/// generated code or (worse) nothing at all, because the declaration was never read. These are the
/// same `const _ { const fn assert_x<T: Bound>() {} }` assertions ID(shape/bound-at-last) already
/// uses for Attr(shape), spanned with quote_spanned! so the error lands on the type the author
/// named rather than on the item or on code they did not write.
///
/// NOTE(#assert-item/needs-the-generics): V[S(const).has(F(generic))], "The check sits in a generic fn, since a const has no generics"
/// The check cannot sit directly in a `const _: () = { .. }`, because a const item HAS NO GENERICS
/// and the source type is written against the stage lifetime - `&'ast Field` there is
/// `error[E0261]: use of undeclared lifetime name`. Wrapping it in a function that carries the
/// type's own generics puts the lifetime back in scope. The function is never called; its BODY is
/// what rustc type-checks, and that is where the bound is proved.
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
