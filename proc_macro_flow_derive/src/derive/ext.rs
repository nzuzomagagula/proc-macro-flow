// @review [ ]
//! Extension traits over the syn types the derives read.
//!
//! NOTE(#derive/helpers-belong-to-types): V[!N(derive).F(free)], "These were free functions taking
//! a syn node as their first argument - `find_one(attrs, name)`, `expr_arg(attr)`,
//! `unwrap_generic(ty, name)` - which is a method with the receiver written out. They are extension
//! TRAITS for the same reason the whole vocab suite is (ID(vocab/orphan-shapes-the-api)): the types
//! are foreign, so an inherent impl is impossible and a local trait is the only way to put the
//! operation where it belongs.
//!
//! This is ID(pipeline/no-free-functions) applied to the derive crate. The argument there was about
//! procedural-macro ergonomics; here it is plainer - `field.named_ident()?` reads as a question
//! about the field, and `named_ident(field)?` reads as a question about nothing in particular"

use syn::{Attribute, Error, Expr, Field, GenericArgument, Ident, PathArguments, Result, Type};

use super::Arity;

/// Reading a declared type: what it wraps, and how many of it there are.
pub(crate) trait TypeExt {
    /// The first generic argument of `Name<..>`, when the type's LAST SEGMENT is `Name`.
    ///
    /// The last segment is the point: `std::option::Option<T>` and `Option<T>` are the same thing
    /// here, which is what a proc macro can do and `macro_rules!` cannot
    /// (NOTE(#syntax-derive/parses-the-type)).
    fn unwrap_generic(&self, name: &str) -> Option<&Type>;

    /// Arity, read off the WRITTEN type and never off an attribute (ID(from/arity-from-type)).
    fn arity(&self) -> Arity;

    /// What a field of this type reads: `Option<T>` and `Vec<T>` read a `T`, anything else itself.
    fn inner(&self) -> &Type;

    /// The `T` of `Extracted<T, I>` — the extractor that produced this child.
    fn extractor(&self) -> Result<Type>;
}

impl TypeExt for Type {
    fn unwrap_generic(&self, name: &str) -> Option<&Type> {
        let Type::Path(path) = self else { return None };
        let segment = path.path.segments.last()?;

        if segment.ident != name {
            return None;
        }

        let PathArguments::AngleBracketed(args) = &segment.arguments else {
            return None;
        };

        args.args.iter().find_map(|arg| match arg {
            GenericArgument::Type(inner) => Some(inner),
            _ => None,
        })
    }

    fn arity(&self) -> Arity {
        if self.unwrap_generic("Vec").is_some() {
            Arity::Many
        } else if self.unwrap_generic("Option").is_some() {
            Arity::Maybe
        } else {
            Arity::One
        }
    }

    fn inner(&self) -> &Type {
        self.unwrap_generic("Option")
            .or_else(|| self.unwrap_generic("Vec"))
            .unwrap_or(self)
    }

    fn extractor(&self) -> Result<Type> {
        self.unwrap_generic("Extracted").cloned().ok_or_else(|| {
            Error::new_spanned(
                self,
                "expected `Extracted<T, I>`, optionally inside `Vec` or `Option` - a field with \
                 `#[from]` holds what its child extractor produced",
            )
        })
    }
}

/// Reading the item a derive was applied to.
pub(crate) trait DeriveInputExt {
    /// The syn node this extraction reads, from `#[source(Ty)]`.
    fn source_type(&self) -> Result<Type>;
}

impl DeriveInputExt for syn::DeriveInput {
    fn source_type(&self) -> Result<Type> {
        let attr = self.attrs.find_one("source")?.ok_or_else(|| {
            Error::new_spanned(
                &self.ident,
                "`#[source(Ty)]` names the syn node this reads - without it a `#[from]` \
                 expression has no typed `source` to be written against",
            )
        })?;

        attr.parse_args::<Type>()
    }
}

/// Reading one attribute.
pub(crate) trait AttributeExt {
    /// The single expression in `#[name(expr)]`.
    ///
    /// The LIST form and not `#[name = expr]`: rustc rejects the name-value form with
    /// `attribute value must be a literal`, so an expression never reaches the macro
    /// (ID(derive/list-not-name-value)).
    fn expr_arg(&self) -> Result<Expr>;
}

impl AttributeExt for Attribute {
    fn expr_arg(&self) -> Result<Expr> {
        self.parse_args::<Expr>().map_err(|_| {
            Error::new_spanned(
                self,
                "expected `(<expression>)` - the expression is spliced verbatim and never inspected",
            )
        })
    }
}

/// Reading a list of attributes.
pub(crate) trait AttributesExt {
    /// At most one attribute with the given head, or an error naming the duplicate.
    fn find_one(&self, name: &str) -> Result<Option<&Attribute>>;
}

impl AttributesExt for [Attribute] {
    fn find_one(&self, name: &str) -> Result<Option<&Attribute>> {
        let mut found = self.iter().filter(|attr| attr.path().is_ident(name));
        let first = found.next();

        if let Some(extra) = found.next() {
            return Err(Error::new_spanned(
                extra,
                format!("`{name}` is written more than once"),
            ));
        }

        Ok(first)
    }
}

/// Reading one field of a struct.
pub(crate) trait FieldExt {
    /// This field's name, as an ERROR rather than a panic when it has none.
    ///
    /// Both derives reach this only after matching `Fields::Named`, so `None` is unreachable - and
    /// an unreachable `.expect()` inside a proc macro is still a panic during the AUTHOR'S compile,
    /// reported as an opaque macro failure with no span. See NOTE(#derive/no-panics).
    fn named_ident(&self) -> Result<&Ident>;
}

impl FieldExt for Field {
    fn named_ident(&self) -> Result<&Ident> {
        self.ident.as_ref().ok_or_else(|| {
            Error::new_spanned(
                self,
                "expected a named field - a tuple struct is all-positional, which is a separate \
                 reading (#positional)",
            )
        })
    }
}
