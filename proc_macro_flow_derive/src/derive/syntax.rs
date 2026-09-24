// @review [ ]
//! `#[derive(Syntax)]` — a grammar type declares itself.
//!
//! This is the bootstrap ID(syntax/derive) named: the derive is what lets a grammar be written as
//! ordinary Rust types instead of as a hand-rolled `meta_list!` invocation, and it is what puts the
//! whole vocabulary suite on the macro path for the first time.
//!
//! NOTE(#syntax-derive/parses-the-type): V[F(derive_syntax).uses(F(Child::of))], "Arity is read by
//! PARSING the field's type and looking at `segments.last()`, reusing F(Child::of) from the
//! extractor derive rather than writing a second reader. That is the difference a proc macro makes
//! and the reason this exists at all: M(meta_list) matches the TOKENS `Option < .. >`, so
//! `std::option::Option<T>` reads as required there (NOTE(#forwarding/no-option) covers why that
//! stays loud). Here it is simply correct, because the type is parsed and a path's last segment is
//! a question the AST can answer"

use heck::{ToKebabCase, ToLowerCamelCase, ToSnakeCase, ToUpperCamelCase};
use quote::{quote, ToTokens};
use syn::{Data, DeriveInput, Error, Fields, ImplItem, Item, ItemImpl, Result, parse2};

use proc_macro_flow_traits::assert::{Assert, AssertKind};
use proc_macro_flow_traits::extractor::{
    Extracted, Extraction, Extractor, Reason, ReasonKind, Validate,
};
use proc_macro_flow_traits::generator::Generator;
use proc_macro_flow_traits::pipeline::Pipeline;
use proc_macro_flow_traits::processor::Processor;
use proc_macro_flow_traits::render::Diagnose;

use super::Arity;
use super::ext::{AttributesExt, FieldExt, TypeExt};

/// A grammar node, as declared.
///
/// NOTE(#syntax-derive/grammar-is-a-type): V[S(Grammar).M(node) && S(Grammar).M(reader)], "The
/// three builders were free functions over `&[Field]` plus whichever other argument each needed -
/// `node_const(entry, fields)`, `reader(name, fields)`, `shape_bounds(fields)`. Three functions
/// sharing a parameter list IS a type, and writing it down means the entry name and the fields
/// cannot be passed in the wrong order or forgotten. ID(derive/helpers-belong-to-types)"
pub(crate) struct Grammar<'ast> {
    /// The type being derived on.
    name: &'ast syn::Ident,
    /// Its entry attribute head - the type name in snake_case, via heck at expansion time.
    entry: String,
    fields: Vec<Field<'ast>>,
    generics: &'ast syn::Generics,
    /// The rules this grammar states about itself, from `#[assert(..)]`.
    rules: Vec<Rule<'ast>>,
}

/// One rule, as written.
///
/// NOTE(#assert/rules-are-checked-at-derive-time): V[F(check).before(emit)], "A rule names KEYS,
/// and whether those keys exist and can be absent is decidable HERE - the derive is looking at the
/// struct. So a rule that names a field which is not there, or a field whose type says it is
/// always present, is a compile error at the attribute rather than a check that can never fire.
///
/// The second of those is ID(from/arity-from-type) in its strongest form: `one_of(a, b)` asks
/// which of two keys was written, and for `a: T` the type has already answered 'always'. The rule
/// is not merely redundant, it is a statement the type contradicts."
pub(crate) enum Rule<'ast> {
    /// A built-in, and the fields it names.
    Builtin {
        kind: AssertKind,
        /// Kept as idents so a complaint can be spanned against the one that is wrong.
        fields: Vec<&'ast syn::Ident>,
        /// The head, for spanning a complaint about the rule as a whole.
        head: syn::Path,
    },
    /// `with = SomeRule` - an author's own.
    With(syn::Path),
}

/// One declared child of a grammar node.
pub(crate) struct Field<'ast> {
    ident: &'ast syn::Ident,
    ty: &'ast syn::Type,
    /// The canonical key, in snake_case. Derived unless the author aliased it.
    key: String,
    /// Extra accepted spellings, canonical excluded.
    aliases: Vec<String>,
    arity: Arity,
    /// The `#[shape(..)]` selector, carried verbatim and never read - ID(no-parse).
    shape: Option<syn::Path>,
}

/// A grammar type as WRITTEN, before anything is derived from it.
///
/// NOTE(#syntax-derive/declaration-before-derivation): V[S(GrammarDeclaration).!computes], "This
/// stage carries what the author wrote and derives nothing from it: the `#[alias]` form rather
/// than the spellings it expands to, the field's type rather than its arity, the rule metas rather
/// than the fields they resolve against. Everything computed - heck's casings, the entry head,
/// arity, and whether a rule names a field that exists and can be absent - is processing, and
/// belongs where the whole type is visible at once.
///
/// Splitting it this way is what lets the three derive-time checks live in one place with the
/// field list in hand, instead of being threaded through a read that is looking at one field."
pub(crate) struct GrammarDeclaration<'ast> {
    name: &'ast syn::Ident,
    generics: &'ast syn::Generics,
    fields: Vec<Extracted<FieldDeclaration<'ast>, &'ast syn::Field>>,
    /// `#[assert(..)]`'s contents, CARRIED. Resolving a rule needs every field, which this stage
    /// cannot see - ID(no-parse) applied to the grammar's own rules.
    rules: Vec<syn::Meta>,
}

/// One field, as written.
pub(crate) struct FieldDeclaration<'ast> {
    ident: &'ast syn::Ident,
    ty: &'ast syn::Type,
    aliases: AliasDeclaration,
    /// The `#[shape(..)]` selector, carried verbatim and never read - ID(no-parse).
    shape: Option<syn::Path>,
}

/// How a field asked to be spelled.
enum AliasDeclaration {
    /// No `#[alias]` at all.
    None,
    /// `#[alias]` bare - the standard case set.
    Standard,
    /// `#[alias("x", "y")]` - exactly these.
    Exactly(Vec<String>),
}

impl<'ast> Validate<'ast> for FieldDeclaration<'ast> {
    type Source = &'ast syn::Field;
    type Valid = &'ast syn::Field;

    fn validate(input: Self::Source) -> ::std::result::Result<Self::Valid, Reason> {
        Ok(input)
    }
}

impl<'ast> Extractor<'ast> for FieldDeclaration<'ast> {
    type Output = Extracted<Self, &'ast syn::Field>;

    fn extract_from(node: &'ast syn::Field) -> Self::Output {
        fn read(node: &syn::Field) -> Extraction<FieldDeclaration<'_>> {
            let ident = match node.named_ident() {
                Ok(ident) => ident,
                Err(error) => return Extraction::failed(Reason::new(ReasonKind::Syntax(error))),
            };

            // `#[alias]` with no arguments asks for the standard case set; `#[alias("x", "y")]`
            // adds exactly what it names. Both become LITERALS later, so matching stays exact per
            // ID(vocabulary/exact) - the generation is a convention, not a normalisation rule
            // applied at match time.
            let aliases = match node.attrs.find_one("alias") {
                Ok(None) => AliasDeclaration::None,
                Ok(Some(attr)) if matches!(attr.meta, syn::Meta::Path(_)) => {
                    AliasDeclaration::Standard
                }
                Ok(Some(attr)) => {
                    match attr.parse_args_with(
                        syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated,
                    ) {
                        Ok(written) => AliasDeclaration::Exactly(
                            written.into_iter().map(|lit| lit.value()).collect(),
                        ),
                        Err(error) => {
                            return Extraction::failed(Reason::new(ReasonKind::Syntax(error)));
                        }
                    }
                }
                Err(error) => return Extraction::failed(Reason::new(ReasonKind::Syntax(error))),
            };

            let shape = match node.attrs.find_one("shape") {
                Ok(None) => None,
                Ok(Some(attr)) => match attr.parse_args::<syn::Path>() {
                    Ok(path) => Some(path),
                    Err(error) => {
                        return Extraction::failed(Reason::new(ReasonKind::Syntax(error)));
                    }
                },
                Err(error) => return Extraction::failed(Reason::new(ReasonKind::Syntax(error))),
            };

            Extraction::value(FieldDeclaration {
                ident,
                ty: &node.ty,
                aliases,
                shape,
            })
        }

        Extracted::new(read(node), node)
    }
}

impl Assert for FieldDeclaration<'_> {}

impl Diagnose for FieldDeclaration<'_> {
    fn diagnose(&self, _: &mut Vec<syn::Error>) {}
}

impl<'ast> Validate<'ast> for GrammarDeclaration<'ast> {
    type Source = &'ast DeriveInput;
    type Valid = &'ast syn::FieldsNamed;

    fn validate(input: Self::Source) -> ::std::result::Result<Self::Valid, Reason> {
        let Data::Struct(data) = &input.data else {
            // enums are `variants!`'s job until ID(syntax/derive-enums)
            return Err(Reason::at(ReasonKind::WrongShape, &input.ident));
        };

        match &data.fields {
            // a tuple struct is all-positional, which is ID(positional)'s separate reading
            Fields::Named(named) => Ok(named),
            _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
        }
    }
}

impl<'ast> Extractor<'ast> for GrammarDeclaration<'ast> {
    type Output = Extracted<Self, &'ast DeriveInput>;

    fn extract_from(node: &'ast DeriveInput) -> Self::Output {
        fn read(node: &DeriveInput) -> Extraction<GrammarDeclaration<'_>> {
            let named = match GrammarDeclaration::validate(node) {
                Ok(named) => named,
                Err(reason) => return Extraction::failed(reason),
            };

            let mut out: Extraction<GrammarDeclaration<'_>> = Extraction::default();
            let mut rules = Vec::new();

            for attr in node.attrs.iter().filter(|attr| attr.path().is_ident("assert")) {
                match attr.parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                ) {
                    Ok(written) => rules.extend(written),
                    Err(error) => out.reasons.push(Reason::new(ReasonKind::Syntax(error))),
                }
            }

            out.value = Some(GrammarDeclaration {
                name: &node.ident,
                generics: &node.generics,
                fields: FieldDeclaration::extract_each(named.named.iter()),
                rules,
            });
            out
        }

        Extracted::new(read(node), node)
    }
}

impl Assert for GrammarDeclaration<'_> {}

impl Diagnose for GrammarDeclaration<'_> {
    fn diagnose(&self, out: &mut Vec<syn::Error>) {
        self.fields.diagnose(out);
    }
}

impl<'ast> Processor<'ast> for GrammarDeclaration<'ast> {
    type Input = Extracted<GrammarDeclaration<'ast>, &'ast DeriveInput>;
    type Output = Grammar<'ast>;

    /// Everything DERIVED from the declaration, which is everything a grammar actually is.
    ///
    /// The casings come from heck at EXPANSION time, so every spelling is a literal by the time it
    /// reaches a match and ID(vocabulary/exact) holds. Arity comes off the written type, which is
    /// where a proc macro beats `macro_rules!` - `std::option::Option<T>` and `Option<T>` are the
    /// same thing here because the type is PARSED (NOTE(#syntax-derive/parses-the-type)).
    fn process(input: Self::Input) -> Extraction<Self::Output> {
        // NOTE(#processor/reasons-are-new-not-inherited).
        let mut out: Extraction<Self::Output> = Extraction::default();

        let Some(value) = input.into_extraction().value else {
            return out;
        };

        let mut fields = Vec::new();
        for child in value.fields {
            let Some(declared) = child.into_extraction().value else {
                continue;
            };

            let canonical = declared.ident.to_string().to_snake_case();
            let aliases = match &declared.aliases {
                AliasDeclaration::None => Vec::new(),
                AliasDeclaration::Standard => Field::standard_cases(&canonical),
                AliasDeclaration::Exactly(written) => written.clone(),
            };

            fields.push(Field {
                ident: declared.ident,
                ty: declared.ty,
                key: canonical,
                aliases,
                arity: declared.ty.arity(),
                shape: declared.shape,
            });
        }

        // The three derive-time checks, and the reason they live HERE: each needs the whole field
        // list, which is what a processor has and an extractor does not.
        // ID(assert/rules-are-checked-at-derive-time).
        let mut rules = Vec::new();
        for meta in value.rules {
            match Rule::read(meta, &fields) {
                Ok(rule) => rules.push(rule),
                Err(error) => out.reasons.push(Reason::new(ReasonKind::Syntax(error))),
            }
        }

        out.value = Some(Grammar {
            name: value.name,
            // heck at EXPANSION time, so the entry head is a literal and matching stays exact.
            entry: value.name.to_string().to_snake_case(),
            fields,
            generics: value.generics,
            rules,
        });
        out
    }
}

/// Four impls and the shape bounds.
pub(crate) struct SyntaxExpansion(ItemImpl, ItemImpl, ItemImpl, ItemImpl, Bounds);

/// One `const _` per `#[shape(..)]` selector - however many the grammar declared.
///
/// A newtype over a Ty(Vec) rather than a counted tuple, and honestly so: the bound SET is one
/// thing whose size the grammar decides, unlike the four impls above, which are always four.
pub(crate) struct Bounds(Vec<Item>);

impl ToTokens for SyntaxExpansion {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.0.to_tokens(tokens);
        self.1.to_tokens(tokens);
        self.2.to_tokens(tokens);
        self.3.to_tokens(tokens);
        self.4.to_tokens(tokens);
    }
}

impl ToTokens for Bounds {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        for item in &self.0 {
            item.to_tokens(tokens);
        }
    }
}

impl<'ast> Generator<'ast> for SyntaxExpansion {
    type Input = Grammar<'ast>;
    type Subject = &'ast DeriveInput;
    type Output = Self;

    fn generate(grammar: Grammar<'ast>) -> Extraction<Self> {
        let name = grammar.name;
        let (impl_generics, type_generics, where_clause) = grammar.generics.split_for_impl();

        let built = (|| -> Result<SyntaxExpansion> {
            let node = grammar.node()?;
            let reader = grammar.reader()?;
            let assert = grammar.assert()?;
            let bounds = grammar.bounds()?;

            // Each item parsed on its own, so a malformed one names the generator that built it
            // rather than arriving in the author's crate - NOTE(#derive/expansion-is-typed-items).
            let described = parse2::<ItemImpl>(quote! {
                impl #impl_generics ::proc_macro_flow_traits::node::Described
                    for #name #type_generics #where_clause
                {
                    #node
                }
            })?;

            // The WORK. See NOTE(#from-body/one-reader-two-entries) for why this is the half that
            // holds it: the reader never wanted the attribute's head, only the tokens inside its
            // delimiters.
            let body = parse2::<ItemImpl>(quote! {
                impl #impl_generics ::proc_macro_flow_traits::vocab::leaves::FromBody
                    for #name #type_generics #where_clause
                {
                    #reader
                }
            })?;

            // The ADAPTER, and the only place `require_list` survives. `meta` is passed as the
            // fallback rather than `meta.span()`, so every span on this path is what it was before
            // the split - NOTE(#from-body/fallback-is-tokens-not-a-span).
            let from_meta = parse2::<ItemImpl>(quote! {
                impl #impl_generics ::proc_macro_flow_traits::vocab::leaves::FromMeta
                    for #name #type_generics #where_clause
                {
                    fn from_meta(meta: &::syn::Meta) -> ::syn::Result<Self> {
                        let list = ::syn::Meta::require_list(meta)?;
                        <Self as ::proc_macro_flow_traits::vocab::leaves::FromBody>::from_body(
                            &list.tokens,
                            meta,
                        )
                    }
                }
            })?;

            let asserts = parse2::<ItemImpl>(quote! {
                impl #impl_generics ::proc_macro_flow_traits::assert::Assert
                    for #name #type_generics #where_clause
                {
                    #assert
                }
            })?;

            Ok(SyntaxExpansion(
                described,
                body,
                from_meta,
                asserts,
                Bounds(bounds),
            ))
        })();

        match built {
            Ok(expansion) => Extraction::value(expansion),
            Err(error) => Extraction::failed(Reason::new(ReasonKind::Internal(error))),
        }
    }

    /// No vacant form - NOTE(#derive/the-impl-is-the-product).
    fn stub(subject: &'ast DeriveInput) -> Result<Self> {
        Err(Error::new_spanned(
            &subject.ident,
            "`#[derive(Syntax)]` describes a struct of NAMED grammar fields - enums are \
             `variants!`'s job until #syntax/derive-enums, and a tuple struct is all-positional",
        ))
    }
}

/// `#[derive(Syntax)]`, wired.
// TODO[ ](#syntax/derive-is-a-pipeline): R[F(derive_syntax) -> S(SyntaxWiring)], "The largest
// split: extraction carries Attr(alias), Attr(shape) and the rule metas AS WRITTEN, and everything
// derived - heck's casings, the entry head, arity, and the three rule checks - is processing,
// which is the only stage that sees every field at once"
pub(crate) struct SyntaxWiring;

impl<'ast> Pipeline<'ast> for SyntaxWiring {
    type Extractor = GrammarDeclaration<'ast>;
    type Processor = GrammarDeclaration<'ast>;
    type Generator = SyntaxExpansion;
}

/// The standard alias set: the same name in the three casings a user might reach for.
///
/// Canonical is excluded - it is already `key`, and a spelling listed twice would show up twice in
/// a did-you-mean list.
impl Field<'_> {
    fn standard_cases(canonical: &str) -> Vec<String> {
        [canonical.to_lower_camel_case(), canonical.to_kebab_case()]
            .into_iter()
            .filter(|spelling| spelling != canonical)
            .collect()
    }
}

impl<'ast> Grammar<'ast> {
    /// The reflection table this node emits.
    fn node(&self) -> Result<ImplItem> {
        let (entry, fields) = (&self.entry, &self.fields);
        let children = fields.iter().map(|field| {
            let key = &field.key;
            let aliases = &field.aliases;
            let arity = match field.arity {
                Arity::One => quote!(Required),
                Arity::Maybe => quote!(Optional),
                Arity::Many => quote!(Repeated),
            };
            let shapes = match &field.shape {
                // The selector names a TYPE, so its runtime identity comes from the Shape impl rather
                // than from anything we compare - NOTE(#shape/two-facts).
                Some(path) => quote!(&[<#path as ::proc_macro_flow_traits::meta::Shape>::KIND]),
                None => quote!(&[]),
            };

            quote! {
                ::proc_macro_flow_traits::node::Child {
                    key: #key,
                    aliases: &[ #(#aliases),* ],
                    arity: ::proc_macro_flow_traits::node::Arity::#arity,
                    shapes: #shapes,
                }
            }
        });

        parse2(quote! {
            const NODE: ::proc_macro_flow_traits::node::Node =
                ::proc_macro_flow_traits::node::Node {
                    name: #entry,
                    children: &[ #(#children),* ],
                };
        })
    }

    /// The `from_body` body: walk the list, read each field, then check what was required.
    ///
    /// Everything a complaint here cannot span itself falls back to `at`, which is `meta` on the
    /// derive path and the ANNOTATED ITEM on the attribute-macro path - see
    /// NOTE(#from-body/fallback-is-tokens-not-a-span).
    fn reader(&self) -> Result<ImplItem> {
        let (name, fields) = (self.name, &self.fields);
        let idents: Vec<&syn::Ident> = fields.iter().map(|field| field.ident).collect();
        let keys: Vec<&String> = fields.iter().map(|field| &field.key).collect();
        let aliases = fields.iter().map(|field| &field.aliases);

        let reads = fields.iter().map(|field| {
            let ident = field.ident;
            let inner = field.ty.inner();
            quote! {
                Key::#ident => {
                    #ident = ::std::option::Option::Some(
                        <#inner as ::proc_macro_flow_traits::vocab::leaves::FromMeta>::from_meta(
                            element,
                        )?,
                    );
                }
            }
        });

        // EVERY missing key is reported, not just the first. The earlier shape returned as soon as it
        // found one, which is the sibling-dropping ID(no-result) exists to prevent - and it reached
        // for `.err().expect("not empty")` to do it, a panic in the AUTHOR'S compile standing on an
        // invariant established two lines away.
        let missing = fields
            .iter()
            .filter(|field| field.arity != Arity::Maybe)
            .map(|field| {
                let ident = field.ident;
                let key = &field.key;
                quote! {
                    if #ident.is_none() {
                        errors.push(::syn::Error::new_spanned(
                            at,
                            ::std::concat!("missing required key `", #key, "`"),
                        ));
                    }
                }
            });

        let takes = fields.iter().map(|field| {
            let ident = field.ident;
            let key = &field.key;
            match field.arity {
                // Field-init SHORTHAND, not `#ident: #ident`. The long form is what
                // `clippy::redundant_field_names` fires on, and a lint in generated code is
                // reported against the AUTHOR's struct - they see a warning about a line they
                // did not write and cannot silence. Generated code owes the same cleanliness as
                // written code; see NOTE(#derive/no-panics) for the same argument about panics.
                Arity::Maybe => quote!( #ident ),
                // Reached only inside the Ok arm, where the check above has already passed - so None
                // would be a FRAMEWORK bug. It bubbles a diagnostic saying so rather than panicking;
                // see NOTE(#derive/no-panics).
                _ => quote! {
                    #ident: match #ident {
                        ::std::option::Option::Some(value) => value,
                        ::std::option::Option::None => {
                            return ::std::result::Result::Err(::syn::Error::new_spanned(
                                at,
                                ::std::concat!(
                                    "internal: `", #key, "` passed the required check and then was \
                                     not present. This is a proc_macro_flow bug."
                                ),
                            ));
                        }
                    }
                },
            }
        });

        parse2(quote! {
            fn from_body<__At>(
                body: &::proc_macro_flow_traits::proc_macro2::TokenStream,
                at: &__At,
            ) -> ::syn::Result<Self>
            where
                __At: ::proc_macro_flow_traits::quote::ToTokens + ?::std::marker::Sized,
            {
                let body = ::proc_macro_flow_traits::meta::ListBody(body);

                // The key set, local to this reader - the same shape meta_list! emits, and for the
                // same reason: it needs no unique name and there is no second public name to keep in
                // step. Unlike meta_list!, aliases are real here, because a proc macro can build the
                // literals.
                ::proc_macro_flow_traits::keys! {
                    #[allow(non_camel_case_types)]
                    enum Key { #( #idents = #keys ),* }
                }
                // The alias spellings the Node table advertises, asserted against the key set so the
                // two cannot drift. TODO[ ](#syntax-derive/aliases-in-keys): `keys!` accepts one
                // spelling per variant, so an alias is currently visible to diagnostics but not to
                // `Keys::resolve`. Extending `keys!` to take `ident = "a" | "b"` closes it.
                const _: &[&[&str]] = &[ #( &[ #(#aliases),* ] ),* ];

                #( let mut #idents = ::std::option::Option::None; )*
                let mut errors = ::proc_macro_flow_traits::vocab::walk::Errors::new();

                errors.absorb(body.walk::<Key, _>(|written, element| {
                    // EXHAUSTIVE over the key set - there is no arm to forget.
                    match written.key() { #(#reads)* }
                    ::std::result::Result::Ok(())
                }));

                #(#missing)*

                match errors.finish() {
                    ::std::result::Result::Err(error) => ::std::result::Result::Err(error),
                    ::std::result::Result::Ok(()) => {
                        ::std::result::Result::Ok(#name { #(#takes),* })
                    }
                }
            }
        })
    }

    /// Step 6: the selector becomes a BOUND.
    ///
    /// NOTE(#shape/bound-at-last): V[Attr(shape).lowers_to(Tr(Shape))], "Ty(Shape) was declared long
    /// before anything used it - `S: Shape` and `S::KIND` appeared only in meta.rs's own tests, so the
    /// promise that Attr(shape) lowers to a trait BOUND rather than a runtime match was recorded and
    /// unbuilt. This is where it is spent: a selector that names a shape the field's type cannot be
    /// read in fails in the AUTHOR's crate, at the author's span.
    ///
    /// The runtime check is untouched and still correct. The two answer different questions -
    /// NOTE(#shape/two-facts) - and this is the half that had never been exercised"
    /// Step 6: the selector becomes a BOUND.
    fn bounds(&self) -> Result<Vec<Item>> {
        let fields = &self.fields;
        let assertions = fields.iter().filter_map(|field| {
        let path = field.shape.as_ref()?;
        let inner = field.ty.inner();
        let span = path.segments.last().map(|s| s.ident.span())?;

        Some(parse2::<Item>(quote::quote_spanned! { span =>
            const _: () = {
                const fn assert_shape<S: ::proc_macro_flow_traits::meta::Shape>() {}
                assert_shape::<#path>();
                const fn assert_readable<T: ::proc_macro_flow_traits::vocab::leaves::FromMeta>() {}
                assert_readable::<#inner>();
            };
        }))
    });

        assertions.collect()
    }
}

// TODO[ ](#assert/derive-reads-rules): C[Attr(assert)] && V[F(Rule::resolve).rejects(required)],
// "Attr(assert) read off the type, with three checks a derive can make and a runtime cannot: the
// field exists, the rule takes that many keys, and - the one that earns it - a REQUIRED field is
// always written, so a rule asking whether it was is a statement its own type contradicts"
impl<'ast> Rule<'ast> {
    fn read(meta: syn::Meta, fields: &[Field<'ast>]) -> Result<Self> {
        match meta {
            // `with = SomeRule`. The value is a path, and nothing here checks what it points at -
            // the emitted call does, against Ty(Rule::Subject).
            syn::Meta::NameValue(pair) if pair.path.is_ident(AssertKind::With.spelling()) => {
                match &pair.value {
                    syn::Expr::Path(path) => Ok(Rule::With(path.path.clone())),
                    other => Err(Error::new_spanned(
                        other,
                        "`with = ..` names a type implementing `Rule`",
                    )),
                }
            }

            // A built-in: `one_of(a, b)`.
            syn::Meta::List(list) => {
                let kind = AssertKind::try_from(&list.path)?;
                if kind == AssertKind::With {
                    return Err(Error::new_spanned(
                        &list.path,
                        "`with` names an author's own rule, so it is written `with = SomeRule`",
                    ));
                }

                let named = list.parse_args_with(
                    syn::punctuated::Punctuated::<syn::Ident, syn::Token![,]>::parse_terminated,
                )?;

                let resolved = named
                    .iter()
                    .map(|name| Rule::resolve(name, fields))
                    .collect::<Result<Vec<_>>>()?;

                Rule::check_arity(kind, &resolved, &list.path)?;

                Ok(Rule::Builtin {
                    kind,
                    fields: resolved,
                    head: list.path.clone(),
                })
            }

            other => Err(Error::new_spanned(
                other,
                format!(
                    "expected a rule - one of: {}, or `with = SomeRule`",
                    AssertKind::candidates()
                ),
            )),
        }
    }

    /// The first two derive-time checks: the key exists, and its presence is a real question.
    fn resolve(name: &syn::Ident, fields: &[Field<'ast>]) -> Result<&'ast syn::Ident> {
        let Some(field) = fields.iter().find(|field| field.ident == name) else {
            let known: Vec<String> = fields
                .iter()
                .map(|field| format!("`{}`", field.ident))
                .collect();
            return Err(Error::new_spanned(
                name,
                format!(
                    "no field named `{name}` - this grammar has {}",
                    known.join(", ")
                ),
            ));
        };

        // ID(assert/rules-are-checked-at-derive-time). A required field is ALWAYS written, so a
        // rule asking whether it was is a statement its own type contradicts.
        if field.arity == Arity::One {
            return Err(Error::new_spanned(
                name,
                format!(
                    "`{name}` is required, so it is always written and a rule about whether it \
                     was cannot fail - make it `Option<..>` if it is genuinely optional"
                ),
            ));
        }

        Ok(field.ident)
    }

    /// The third: a rule given the wrong number of keys.
    fn check_arity(kind: AssertKind, fields: &[&syn::Ident], head: &syn::Path) -> Result<()> {
        let (least, most) = match kind {
            AssertKind::Requires => (2, Some(2)),
            _ => (2, None),
        };

        if fields.len() < least || most.is_some_and(|most| fields.len() > most) {
            let wanted = match most {
                Some(most) if most == least => format!("exactly {least}"),
                _ => format!("at least {least}"),
            };
            return Err(Error::new_spanned(
                head,
                format!(
                    "`{kind}` takes {wanted} keys, and {} {} named",
                    fields.len(),
                    if fields.len() == 1 { "was" } else { "were" }
                ),
            ));
        }

        Ok(())
    }
}

impl Grammar<'_> {
    /// The `assert` body: this grammar's own rules, then a descent into every field.
    ///
    /// The descent is unconditional and needs no knowledge of which fields are grammars, because
    /// every leaf is askable too - NOTE(#assert/leaves-are-askable).
    fn assert(&self) -> Result<ImplItem> {
        let checks = self
            .rules
            .iter()
            .map(|rule| rule.emit(&self.fields))
            .collect::<Result<Vec<syn::Stmt>>>()?;
        let descend = self.fields.iter().map(|field| {
            let ident = field.ident;
            quote!(::proc_macro_flow_traits::assert::Assert::assert(&self.#ident, out);)
        });

        parse2(quote! {
            fn assert(&self, out: &mut ::std::vec::Vec<::proc_macro_flow_traits::extractor::Reason>) {
                #(#checks)*
                #(#descend)*
            }
        })
    }
}

impl Field<'_> {
    /// Whether this key was WRITTEN, read off the arity the type already stated.
    ///
    /// `Arity::One` never reaches here - a required key is always written, and
    /// ID(assert/rules-are-checked-at-derive-time) rejects a rule naming one.
    fn was_written(&self) -> Result<syn::Expr> {
        let ident = self.ident;
        parse2(match self.arity {
            Arity::Maybe => quote!(::std::option::Option::is_some(&self.#ident)),
            Arity::Many => quote!(!::std::vec::Vec::is_empty(&self.#ident)),
            Arity::One => quote!(true),
        })
    }
}

impl Rule<'_> {
    /// The check itself.
    ///
    /// Each one records a E(Reason) and carries on - there is no `?` and no early return, so a
    /// grammar stating three rules reports all three it breaks rather than the first
    /// (NOTE(#assert/no-result)).
    fn emit(&self, fields: &[Field<'_>]) -> Result<syn::Stmt> {
        let (kind, named, _) = match self {
            // An author's rule words its OWN reason, so nothing is built here -
            // NOTE(#assert/with-never-violates).
            Rule::With(path) => {
                return parse2(quote! {
                    <#path as ::proc_macro_flow_traits::assert::Rule>::check(self, out);
                });
            }
            Rule::Builtin { kind, fields, head } => (kind, fields, head),
        };

        // Resolved when the rule was read, so a miss here cannot happen on a grammar that got
        // this far - and it still bubbles rather than panicking (NOTE(#derive/no-panics)).
        let presence = named
            .iter()
            .map(|name| match fields.iter().find(|field| field.ident == *name) {
                Some(field) => field.was_written(),
                None => Err(Error::new_spanned(
                    name,
                    "internal: a rule named a field that resolved earlier and cannot be found \
                     now. This is a proc_macro_flow bug.",
                )),
            })
            .collect::<Result<Vec<syn::Expr>>>()?;

        // The VARIANT ident, derived from the vocabulary's own spelling rather than written out
        // again - `one_of` becomes `OneOf`, `at_most_one` becomes `AtMostOne`. A second table
        // mapping kinds to idents could disagree with the first; this cannot.
        let variant = quote::format_ident!("{}", kind.spelling().to_upper_camel_case());

        let keys = named.iter().map(|name| name.to_string());
        let violation = quote! {
            out.push(::proc_macro_flow_traits::extractor::Reason::new(
                ::proc_macro_flow_traits::extractor::ReasonKind::Violated(
                    ::proc_macro_flow_traits::assert::Violation::new(
                        ::proc_macro_flow_traits::assert::AssertKind::#variant,
                        &[ #(#keys),* ],
                    ),
                ),
            ));
        };

        // `requires` asks about ONE key's effect on another, so it is not a count at all.
        if let AssertKind::Requires = kind {
            let mut presence = presence.into_iter();
            let (first, second) = (presence.next(), presence.next());
            return parse2(quote! {
                if #first && !(#second) {
                    #violation
                }
            });
        }

        let test = match kind {
            AssertKind::OneOf => quote!(written != 1),
            AssertKind::AnyOf => quote!(written == 0),
            // `conflicts` is the same COUNT as `at_most_one` and a different thing to say about
            // it - "these do not go together" rather than "pick one". The wording is what differs,
            // and E(Violation) carries the rule so the wording can.
            AssertKind::AtMostOne | AssertKind::Conflicts => quote!(written > 1),
            AssertKind::Requires | AssertKind::With => quote!(false),
        };

        parse2(quote! {
            {
                let written = [ #(#presence),* ]
                    .into_iter()
                    .filter(|value: &bool| *value)
                    .count();
                if #test {
                    #violation
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;
    use syn::parse_str;

    /// Drive the WHOLE pipeline, exactly as the entry function does.
    fn expand(source: &str) -> String {
        let input: DeriveInput = parse_str(source).expect("the item parses");
        SyntaxWiring::run(&input).to_token_stream().to_string()
    }

    /// The same, asserting the grammar was refused.
    ///
    /// A pipeline does not return an error - it emits one, beside whatever it could still build.
    /// So a rejection is read out of the OUTPUT now, which is also what the author sees.
    fn rejected(source: &str) -> String {
        let out = expand(source);
        assert!(out.contains("compile_error"), "the derive accepted this: {out}");
        out
    }

    #[test]
    fn a_grammar_with_no_rules_still_gets_an_assert_that_descends() {
        // The descent is what carries a NESTED grammar's rules up, so it is emitted whether or not
        // this type states any of its own.
        let out = expand("struct Retry { times: Option<LitInt> }");

        assert!(out.contains("Assert for Retry"), "{out}");
        assert!(out.contains("Assert :: assert (& self . times"), "{out}");
    }

    #[test]
    fn a_rule_becomes_a_check_against_the_arity_the_type_declared() {
        let out = expand(
            "#[assert(one_of(times, forever))] struct Retry { times: Option<LitInt>, forever: Option<LitBool> }",
        );

        assert!(out.contains("AssertKind :: OneOf"), "{out}");
        // Presence read off the TYPE - Option asks is_some, and nothing declared it.
        assert!(out.contains("Option :: is_some (& self . times)"), "{out}");
    }

    #[test]
    fn a_repeated_field_asks_whether_it_is_empty() {
        let out = expand("#[assert(any_of(a, b))] struct G { a: Vec<LitStr>, b: Vec<LitStr> }")
            ;

        assert!(out.contains("! :: std :: vec :: Vec :: is_empty (& self . a)"), "{out}");
    }

    #[test]
    fn a_rule_naming_a_field_that_is_not_there_is_rejected() {
        // ID(assert/rules-are-checked-at-derive-time), first check.
        let message = rejected(
            "#[assert(one_of(times, forevr))] struct Retry { times: Option<LitInt>, forever: Option<LitBool> }",
        );

        assert!(message.contains("no field named `forevr`"), "{message}");
        // And it says what there IS, which is the half that makes it actionable.
        assert!(message.contains("`times`"), "{message}");
        assert!(message.contains("`forever`"), "{message}");
    }

    #[test]
    fn a_rule_naming_a_required_field_is_rejected() {
        // The second check, and the one that earns this design: `times: LitInt` is ALWAYS written,
        // so asking whether it was is a statement its own type contradicts - ID(from/arity-from-type).
        let message = rejected(
            "#[assert(one_of(times, forever))] struct Retry { times: LitInt, forever: Option<LitBool> }",
        );

        assert!(message.contains("`times` is required"), "{message}");
        assert!(message.contains("Option"), "the fix must be named: {message}");
    }

    #[test]
    fn a_rule_given_the_wrong_number_of_keys_is_rejected() {
        // The third check. `requires` is a relation between exactly two keys.
        let three = rejected(
            "#[assert(requires(a, b, c))] struct G { a: Option<LitStr>, b: Option<LitStr>, c: Option<LitStr> }",
        );
        assert!(three.contains("exactly 2"), "{three}");

        let one = rejected("#[assert(one_of(a))] struct G { a: Option<LitStr> }");
        assert!(one.contains("at least 2"), "{one}");
        assert!(one.contains("1 was named"), "{one}");
    }

    #[test]
    fn a_rule_we_do_not_have_is_named_with_the_ones_we_do() {
        let message = rejected(
            "#[assert(exactly_two(a, b))] struct G { a: Option<LitStr>, b: Option<LitStr> }",
        );

        assert!(message.contains("one_of"), "{message}");
    }

    #[test]
    fn with_written_as_a_call_is_corrected() {
        // `with` names a type rather than keys, so `with(Rule)` is a plausible slip worth catching
        // precisely instead of failing as 'no field named Rule'.
        let message = rejected("#[assert(with(SomeRule))] struct G { a: Option<LitStr> }");

        assert!(message.contains("`with = SomeRule`"), "{message}");
    }

    #[test]
    fn an_authors_rule_is_called_against_self() {
        let out = expand("#[assert(with = NoZeroRetries)] struct G { a: Option<LitStr> }")
            ;

        assert!(out.contains("NoZeroRetries as :: proc_macro_flow_traits :: assert :: Rule"), "{out}");
        assert!(out.contains(":: check (self , out)"), "{out}");
    }
}
