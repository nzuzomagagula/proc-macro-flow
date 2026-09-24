// @review [ ]
//! What `#[derive(Diagnose)]` builds: the walk, and the `Assert` that asks what it walks.

use proc_macro_flow_traits::extractor::{Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::generator::Generator;
use quote::{quote, ToTokens};
use syn::{parse2, DeriveInput, ItemImpl};

use super::processor::ProcessedWalk;

/// Two impls, and the type says two.
pub(crate) struct WalkExpansion(ItemImpl, ItemImpl);

impl ToTokens for WalkExpansion {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
        self.1.to_tokens(tokens);
    }
}

impl<'ast> Generator<'ast> for WalkExpansion {
    type Input = ProcessedWalk<'ast>;
    type Subject = &'ast DeriveInput;
    type Output = Self;

    /// NOTE(#diagnose-derive/walk-all-and-skip): V[F(generate).has(visits unless skipped)], "Every field is walked unless marked; skip is the cheap mistake"
    /// Every field is walked unless it says otherwise, and the default is that way round on
    /// purpose.
    ///
    /// The two failures are not symmetric. A field wrongly walked is a COMPILE ERROR - `the trait
    /// bound &Ident: Diagnose is not satisfied` - which the author fixes by writing Attr(skip). A
    /// field wrongly NOT walked compiles perfectly and silently loses every diagnostic beneath it,
    /// which is the exact harm ID(derive/diagnose-rides-along) exists to prevent.
    ///
    /// So the mark goes on the CHEAP mistake. Contrast Attr(value) on the extractor derive, where
    /// the child is marked: there both routes compile, so neither failure is loud and the
    /// declaration has to carry the meaning.
    fn generate(input: ProcessedWalk<'ast>) -> Extraction<Self> {
        let name = input.name;
        let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();

        let walked = || input.fields.iter().filter(|(_, walked)| *walked);
        let visits = walked().map(|(ident, _)| {
            quote!(::proc_macro_flow_traits::render::Diagnose::diagnose(&self.#ident, out);)
        });
        let asks = walked().map(|(ident, _)| {
            quote!(::proc_macro_flow_traits::assert::Assert::assert(&self.#ident, out);)
        });

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

        // Tr(Assert) is Tr(Diagnose)'s supertrait, so a type that has one owes the other.
        //
        // TODO[x](#assert/walked-fields-are-asked): U[F(generate).has(asks walked fields)], "derive(Diagnose) emitted an empty Assert, so a held grammar never had its rules asked"
        // Found through the demo: `#[column(skip, key)]` compiled clean although Column states
        // `conflicts(skip, key)`. Closed by the Assert below together with
        // ID(diagnose/values-are-walkable), which is what lets a grammar field go unskipped.
        //
        // NOTE(#diagnose-derive/asks-what-it-walks): V[F(generate).has(asks what it walks)], "The derived Assert asks every field the walk visits"
        // The Assert descends into the same fields the walk does, and it was EMPTY until the demo's
        // `conflicts(skip, key)` was found never firing. The type states no rules of its own, but a
        // VALUE it holds may - a grammar - and Tr(Assert) is the walk that reaches INTO values.
        // Only the S(Extracted) boundary answers empty, which is what keeps each rule reported
        // exactly once: an Extracted field is asked and says nothing, then walked, and its own
        // value is asked there - ID(assert/extracted-is-the-handoff).
        let assert = parse2::<ItemImpl>(quote! {
            impl #impl_generics ::proc_macro_flow_traits::assert::Assert
                for #name #type_generics #where_clause
            {
                fn assert(
                    &self,
                    out: &mut ::std::vec::Vec<::proc_macro_flow_traits::extractor::Reason>,
                ) {
                    #(#asks)*
                }
            }
        });

        match (diagnose, assert) {
            (Ok(diagnose), Ok(assert)) => Extraction::value(WalkExpansion(diagnose, assert)),
            (Err(error), _) | (_, Err(error)) => {
                Extraction::failed(Reason::new(ReasonKind::Internal(error)))
            }
        }
    }

    /// No vacant form - ID(derive/the-impl-is-the-product).
    fn stub(subject: &'ast DeriveInput) -> syn::Result<Self> {
        Err(syn::Error::new_spanned(
            &subject.ident,
            "`#[derive(Diagnose)]` describes a struct of named fields - it writes the walk over \
             them, and `#[skip]` marks one it should not descend into",
        ))
    }
}
