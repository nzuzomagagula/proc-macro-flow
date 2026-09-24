// @review [ ]
//! A grammar type as WRITTEN: its fields' attributes and its rule metas, carried verbatim.

use proc_macro_flow_traits::assert::Assert;
use proc_macro_flow_traits::extractor::{
    Extracted, Extraction, Extractor, Reason, ReasonKind, Validate,
};
use proc_macro_flow_traits::render::Diagnose;
use syn::{Data, DeriveInput, Fields};

use super::super::ext::{AttributesExt, FieldExt};

/// A grammar type as WRITTEN, before anything is derived from it.
///
/// NOTE(#syntax-derive/declaration-before-derivation): V[S(GrammarDeclaration) != computes], "Declaration carries what was written; derivation happens later"
/// This stage carries what the author wrote and derives nothing from it: the `#[alias]` form rather
/// than the spellings it expands to, the field's type rather than its arity, the rule metas rather
/// than the fields they resolve against. Everything computed - heck's casings, the entry head,
/// arity, and whether a rule names a field that exists and can be absent - is processing, and
/// belongs where the whole type is visible at once.
///
/// Splitting it this way is what lets the three derive-time checks live in one place with the
/// field list in hand, instead of being threaded through a read that is looking at one field.
pub(crate) struct GrammarDeclaration<'ast> {
    pub(crate) name: &'ast syn::Ident,
    pub(crate) generics: &'ast syn::Generics,
    pub(crate) fields: Vec<Extracted<FieldDeclaration<'ast>, &'ast syn::Field>>,
    /// `#[assert(..)]`'s contents, CARRIED. Resolving a rule needs every field, which this stage
    /// cannot see - ID(no-parse) applied to the grammar's own rules.
    pub(crate) rules: Vec<syn::Meta>,
}

/// One field, as written.
pub(crate) struct FieldDeclaration<'ast> {
    pub(crate) ident: &'ast syn::Ident,
    pub(crate) ty: &'ast syn::Type,
    pub(crate) aliases: AliasDeclaration,
    /// The `#[shape(..)]` selector, carried verbatim and never read - ID(no-parse).
    pub(crate) shape: Option<syn::Path>,
}

/// How a field asked to be spelled.
pub(crate) enum AliasDeclaration {
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
