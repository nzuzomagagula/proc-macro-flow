// @review [ ]
//! `#[derive(Processor)]` — the identity.
//!
//! Most extractions copy their fields through and generate from them with no transformation, so
//! the default has to cost nothing. Deriving this IS the opt-in: a type needing real processing
//! omits the derive and writes `impl Processor` by hand, which is ordinary Rust and needs no
//! opt-out attribute.

use quote::quote;
use syn::{DeriveInput, Error, Item, ItemImpl, Result, parse2};

use super::ext::DeriveInputExt;

pub(crate) fn derive_processor(input: DeriveInput) -> Result<Vec<Item>> {
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
        impl #impl_generics ::proc_macro_flow_traits::processor::Processor
            for #name #type_generics #where_clause
        {
            type Input = ::proc_macro_flow_traits::extractor::Extracted<
                Self,
                & #lifetime #source,
            >;
            type Output = Self;

            /// Identity: the extraction IS the processed value. Children are already `Extracted`
            /// inside it, so there is nothing to cascade at this level - a processor that needs to
            /// reach them is doing real work and writes itself.
            fn process(
                input: Self::Input,
            ) -> ::proc_macro_flow_traits::extractor::Extraction<Self::Output> {
                input.into_extraction()
            }
        }
    })?;

    Ok(vec![Item::Impl(item)])
}
