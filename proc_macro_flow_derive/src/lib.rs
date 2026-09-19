// @review [~]
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

use crate::base::extractor::StructExtraction;
use crate::traits::extractor::Extractor;

mod base;


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
    let extraction = StructExtraction::extract_from(&derive_input);

    // The extraction is READ now rather than built and thrown away. What is emitted is still a
    // stub, but the reasons are no longer silently dropped on the floor.
    // TODO[~](#extractor/macro):U[F(extractor)], "Two halves left. (1) The expansion itself is a
    // stub until ExtractorPipeline::expand exists. (2) Only this node's OWN reasons are rendered -
    // children hold theirs, so the recursive descent is ID(syntax/render)'s single final walk,
    // which also has to sort by span and emit a stub impl ALONGSIDE the errors so a missing impl
    // does not cascade into 'does not implement' at every use site and bury the real diagnostic"
    let errors = extraction
        .reasons()
        .iter()
        .map(|reason| {
            reason
                .to_error(extraction.source(), "could not extract")
                .to_compile_error()
        });

    quote! { #(#errors)* }.into()
}
