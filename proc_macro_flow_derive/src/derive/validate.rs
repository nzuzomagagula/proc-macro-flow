// @review [ ]
//! `#[derive(Validate)]` — the trivial pass-through.
//!
//! Separate from `#[derive(Extractor)]` because a real narrowing validate is the interesting case:
//! `StructExtraction` turns a `DeriveInput` into a `&DataStruct`, and a derive that always emitted
//! a trivial one would be unusable for exactly the type that motivated the design. Derive this when
//! there is nothing to check; write it by hand when there is.

use quote::quote;
use syn::{DeriveInput, Error, Item, ItemImpl, Result, parse2};

use super::ext::DeriveInputExt;

pub(crate) fn derive_validate(input: DeriveInput) -> Result<Vec<Item>> {
    let name = &input.ident;

    let source = input.source_type()?;

    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let lifetime = input
        .generics
        .lifetimes()
        .next()
        .map(|def| &def.lifetime)
        .ok_or_else(|| Error::new_spanned(&input.ident, "expected a lifetime parameter"))?;

    let item = parse2::<ItemImpl>(quote! {
        impl #impl_generics ::proc_macro_flow_traits::extractor::Validate<#lifetime>
            for #name #type_generics #where_clause
        {
            // Attr(source(Ty)) maps ONE-FOR-ONE onto the associated type - see
            // NOTE(#pipeline/source-is-associated).
            type Source = & #lifetime #source;
            type Valid = & #lifetime #source;

            /// Narrows nothing, and is honest about it: surface-level validation is what this
            /// trait is for, and this type has no surface check to make.
            fn validate(
                input: & #lifetime #source,
            ) -> ::std::result::Result<Self::Valid, ::proc_macro_flow_traits::extractor::Reason> {
                ::std::result::Result::Ok(input)
            }
        }
    })?;

    Ok(vec![Item::Impl(item)])
}
