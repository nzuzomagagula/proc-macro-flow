// @review [ ]
//! `#[derive(Validate)]` — the trivial pass-through.
//!
//! Separate from `#[derive(Extractor)]` because a real narrowing validate is the interesting case:
//! `StructExtraction` turns a `DeriveInput` into a `&DataStruct`, and a derive that always emitted
//! a trivial one would be unusable for exactly the type that motivated the design. Derive this when
//! there is nothing to check; write it by hand when there is.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{DeriveInput, Error, Result, Type};

use super::find_one;

pub(crate) fn derive_validate(input: DeriveInput) -> Result<TokenStream> {
    let name = &input.ident;

    let attr = find_one(&input.attrs, "source")?.ok_or_else(|| {
        Error::new_spanned(
            &input.ident,
            "`#[derive(Validate)]` needs `#[source(Ty)]` to know what it is validating",
        )
    })?;
    let source: Type = attr.parse_args()?;

    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let lifetime = input
        .generics
        .lifetimes()
        .next()
        .map(|def| &def.lifetime)
        .ok_or_else(|| Error::new_spanned(&input.ident, "expected a lifetime parameter"))?;

    Ok(quote! {
        impl #impl_generics ::proc_macro_flow_traits::extractor::Validate<
            #lifetime,
            & #lifetime #source,
        > for #name #type_generics #where_clause {
            type ValidityError = ();
            type Valid = & #lifetime #source;

            /// Narrows nothing, and is honest about it: surface-level validation is what this
            /// trait is for, and this type has no surface check to make.
            fn validate(
                input: & #lifetime #source,
            ) -> ::std::result::Result<Self::Valid, Self::ValidityError> {
                ::std::result::Result::Ok(input)
            }
        }
    })
}
