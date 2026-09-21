// @review [ ]
//! `#[derive(Extractor)]` — generate `extract_from` from `#[source(..)]` and `#[from]` / `#[with]`.
//!
//! NOTE(#derive/diagnose-rides-along): V[F(derive_extractor).emits(Impl(Diagnose))], "Tr(Diagnose)
//! is emitted HERE rather than as a fourth derive, and that does not contradict
//! ID(derive/three-not-one). Those three are separate because each has a real hand-written case:
//! Validate narrows, Processor does work. Tr(Diagnose) has none - it answers only 'where are my
//! children', and for a struct whose every field is a declared child the field list IS the answer,
//! with no alternative an author could want instead. A derive that always emits the same correct
//! thing should not be opt-in; making it one would just be a way to forget it, and a forgotten
//! Tr(Diagnose) is a silently unreachable subtree rather than a compile error"

use quote::quote;
use syn::{Item, Data, DeriveInput, Error, Fields, ItemImpl, Result, Type, parse2};

use super::{Arity, Child, expr_arg, find_one, named_ident};

pub(crate) fn derive_extractor(input: DeriveInput) -> Result<Vec<Item>> {
    let name = &input.ident;
    let source = source_type(&input)?;

    let Data::Struct(data) = &input.data else {
        return Err(Error::new_spanned(
            &input.ident,
            "`#[derive(Extractor)]` describes a struct of extracted children",
        ));
    };

    let Fields::Named(fields) = &data.fields else {
        return Err(Error::new_spanned(
            &input.ident,
            "`#[derive(Extractor)]` needs named fields - each one names where it comes from",
        ));
    };

    let mut assignments = Vec::new();
    let mut visits = Vec::new();
    for field in &fields.named {
        let ident = named_ident(field)?;
        let from = find_one(&field.attrs, "from")?;
        let with = find_one(&field.attrs, "with")?;

        let reach = match (from, with) {
            (Some(_), Some(other)) => {
                return Err(Error::new_spanned(
                    other,
                    "`#[from(..)]` and `#[with(..)]` are alternatives - `from` is an expression \
                     evaluated with `source` in scope, `with` is a callable applied to it",
                ));
            }
            // Spliced verbatim, never inspected. A bad one is rustc's error at the author's span.
            (Some(from), None) => {
                let expr = expr_arg(from)?;
                quote!(#expr)
            }
            (None, Some(with)) => {
                let expr = expr_arg(with)?;
                quote!((#expr)(source))
            }
            (None, None) => {
                return Err(Error::new_spanned(
                    ident,
                    "every field needs `#[from(..)]` or `#[with(..)]` - an extraction declares \
                     WHERE each child comes from",
                ));
            }
        };

        let child = Child::of(&field.ty)?;
        let extractor = &child.extractor;

        // Arity picks the method, read off the field's TYPE and never off the attribute. These
        // are provided methods on Tr(Extractor), so the extractor NAMES itself and no turbofish is
        // needed - see Fix[x](#from/names-its-target).
        let call = match child.arity {
            Arity::One => quote!( <#extractor>::extract_from(#reach) ),
            Arity::Many => quote!( <#extractor>::extract_each(#reach) ),
            Arity::Maybe => quote!( <#extractor>::extract_maybe(#reach) ),
        };

        assignments.push(quote!(#ident: #call));
        // Every field is a child by construction - reaching it is the whole reason it is declared.
        visits.push(quote!(
            ::proc_macro_flow_traits::render::Diagnose::diagnose(&self.#ident, out);
        ));
    }

    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let lifetime = input
        .generics
        .lifetimes()
        .next()
        .map(|def| &def.lifetime)
        .ok_or_else(|| {
            Error::new_spanned(
                &input.ident,
                "an extraction borrows from the AST, so it needs a lifetime parameter",
            )
        })?;

    let extractor = parse2::<ItemImpl>(quote! {
        impl #impl_generics ::proc_macro_flow_traits::extractor::Extractor<#lifetime>
            for #name #type_generics #where_clause
        {
            type Output = ::proc_macro_flow_traits::extractor::Extracted<
                Self,
                & #lifetime #source,
            >;

            fn extract_from(node: & #lifetime #source) -> Self::Output {
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
                    Ok(source) => ::proc_macro_flow_traits::extractor::Extraction::value(Self {
                        #(#assignments),*
                    }),
                    // Fix[x](#derive/silent-validate): this arm used to be
                    // `Extraction::default()` - no value AND NO REASONS - so a derived extractor
                    // whose validate failed emitted the stub and NOTHING ELSE. The author saw an
                    // impl with no explanation of why it was vacant. It went unnoticed because the
                    // proof test asserted only that the value was absent, never that a reason was
                    // recorded. A derive always records, per
                    // NOTE(#validate/reason-is-offered-not-imposed).
                    Err(reason) => {
                        ::proc_macro_flow_traits::extractor::Extraction::failed(reason)
                    }
                };

                // The source rides on the OUTPUT, so it survives the Err arm where there is no
                // Self to ask.
                ::proc_macro_flow_traits::extractor::Extracted::new(extraction, node)
            }
        }

    })?;

    // Where the children are, and nothing else - the `Extracted` around each one renders its
    // reasons, because it holds the node they span against (ID(render/who-renders)).
    //
    // Parsed SEPARATELY: `parse2::<ItemImpl>` consumes its whole input, so two impls in one call
    // is an error rather than two items. See NOTE(#derive/expansion-is-typed-items).
    let diagnose = parse2::<ItemImpl>(quote! {
        impl #impl_generics ::proc_macro_flow_traits::render::Diagnose
            for #name #type_generics #where_clause
        {
            fn diagnose(&self, out: &mut ::std::vec::Vec<::proc_macro_flow_traits::syn::Error>) {
                #(#visits)*
            }
        }
    })?;

    Ok(vec![Item::Impl(extractor), Item::Impl(diagnose)])
}

/// The syn node this extraction reads, from `#[source(Ty)]`.
fn source_type(input: &DeriveInput) -> Result<Type> {
    let attr = find_one(&input.attrs, "source")?.ok_or_else(|| {
        Error::new_spanned(
            &input.ident,
            "`#[derive(Extractor)]` needs `#[source(Ty)]` naming the syn node it reads - without \
             it a `#[from]` expression has no typed `source` to be written against",
        )
    })?;

    attr.parse_args::<Type>()
}
