// @review [ ]
//! What `#[derive(Extractor)]` builds: three impls, and the type says three.

use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use quote::{quote, ToTokens};
use syn::{parse2, DeriveInput, ItemImpl};

use super::processor::ProcessedExtractor;

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

        let assignments = input.fields.iter().map(|field| {
            let (ident, call) = (field.ident, &field.call);
            quote!(#ident: #call)
        });
        // Only the fields that hold CHILDREN. A `#[value]` field is ordinary data - it does not
        // implement Tr(Diagnose) and there is nothing beneath it to reach.
        let visits = input.fields.iter().filter(|field| field.walked).map(|field| {
            let ident = field.ident;
            quote!(::proc_macro_flow_traits::render::Diagnose::diagnose(&self.#ident, out);)
        });

        let node_type = match &stage.args {
            None => quote!(& #lifetime #source),
            Some(args) => quote! {
                ::proc_macro_flow_traits::attributed::Attributed<#lifetime, #args, #source>
            },
        };

        // Parsed SEPARATELY: `parse2::<ItemImpl>` consumes its whole input, so two impls in one
        // call is an error rather than two items. See ID(derive/expansion-is-typed-items).
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
        // walk instead - ID(assert/extracted-is-the-handoff).
        //
        // TODO[ ](#assert/extractor-values-are-asked): U[Impl(Assert).has(value fields)], "derive(Extractor) never asks its #[value] fields, so a grammar held there loses its rules"
        // 'Every field is an S(Extracted)' stopped being true with ID(extractor/fields-may-hold-values):
        // a `#[value]` field is plain data, and if that data is a grammar its rules are never asked -
        // the gap ID(assert/walked-fields-are-asked) closed for derive(Diagnose). Descending needs
        // every `#[value]` type to be askable, and a borrowed `&Ident` is not yet.
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

    /// No vacant form - ID(derive/the-impl-is-the-product).
    fn stub(subject: &'ast DeriveInput) -> syn::Result<Self> {
        Err(syn::Error::new_spanned(
            &subject.ident,
            "`#[derive(Extractor)]` needs `#[source(Ty)]` and a `#[from(..)]` or `#[with(..)]` on \
             every field before it can write an impl",
        ))
    }
}
