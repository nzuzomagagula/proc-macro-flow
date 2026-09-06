// @review [~]
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input, visit::Visit};

use crate::base::extractor::{ExtractionState, StructExtraction};

mod base;
mod traits;

#[proc_macro_derive(HelloMacro)]
pub fn hello_macro_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let name = ast.ident;

    let expanded = quote! {
        impl #name {
            pub fn hello_macro() {
                println!("Hello, Macro! My name is {}!", stringify!(#name));
            }
        }
    };

    expanded.into()
}

#[proc_macro_derive(Extractor)]
pub fn extractor(input: TokenStream) -> TokenStream {
    let derive_input = parse_macro_input!(input as DeriveInput);
    let mut extractor = ExtractionState::<StructExtraction>::Uninitialised;
    extractor.visit_derive_input(&derive_input);
    // TODO[~](#extractor/macro):U[F(extractor)], "Stub return so the crate compiles - replace with the real expansion once ExtractorPipeline::expand exists (see base/extractor/extractor/mod.rs)"
    TokenStream::new()
}
