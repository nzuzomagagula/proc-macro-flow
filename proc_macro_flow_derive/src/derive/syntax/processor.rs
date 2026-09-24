// @review [ ]
//! Everything DERIVED from the declaration: casings, arity, the entry head, and the rule checks.

use heck::{ToKebabCase, ToLowerCamelCase, ToSnakeCase};
use proc_macro_flow_traits::assert::AssertKind;
use proc_macro_flow_traits::extractor::{Extracted, Extraction, Reason, ReasonKind};
use proc_macro_flow_traits::processor::Processor;
use syn::{DeriveInput, Error, Result};

use super::super::Arity;
use super::super::ext::TypeExt;
use super::extractor::{AliasDeclaration, GrammarDeclaration};

/// A grammar node, as declared.
///
/// NOTE(#syntax-derive/grammar-is-a-type): V[S(Grammar).M(node) && S(Grammar).M(reader)], "The
/// three builders were free functions over `&[Field]` plus whichever other argument each needed -
/// `node_const(entry, fields)`, `reader(name, fields)`, `shape_bounds(fields)`. Three functions
/// sharing a parameter list IS a type, and writing it down means the entry name and the fields
/// cannot be passed in the wrong order or forgotten. ID(derive/helpers-belong-to-types)"
pub(crate) struct Grammar<'ast> {
    /// The type being derived on.
    pub(crate) name: &'ast syn::Ident,
    /// Its entry attribute head - the type name in snake_case, via heck at expansion time.
    pub(crate) entry: String,
    pub(crate) fields: Vec<Field<'ast>>,
    pub(crate) generics: &'ast syn::Generics,
    /// The rules this grammar states about itself, from `#[assert(..)]`.
    pub(crate) rules: Vec<Rule<'ast>>,
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
    pub(crate) ident: &'ast syn::Ident,
    pub(crate) ty: &'ast syn::Type,
    /// The canonical key, in snake_case. Derived unless the author aliased it.
    pub(crate) key: String,
    /// Extra accepted spellings, canonical excluded.
    pub(crate) aliases: Vec<String>,
    pub(crate) arity: Arity,
    /// The `#[shape(..)]` selector, carried verbatim and never read - ID(no-parse).
    pub(crate) shape: Option<syn::Path>,
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

/// The standard alias set: the same name in the three casings a user might reach for.
///
/// Canonical is excluded - it is already `key`, and a spelling listed twice would show up twice in
/// a did-you-mean list.
impl Field<'_> {
    pub(crate) fn standard_cases(canonical: &str) -> Vec<String> {
        [canonical.to_lower_camel_case(), canonical.to_kebab_case()]
            .into_iter()
            .filter(|spelling| spelling != canonical)
            .collect()
    }
}

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
