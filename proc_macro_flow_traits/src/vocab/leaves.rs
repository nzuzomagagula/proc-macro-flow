// @review [ ]
//! `FromExpr` and `leaf!` - reading a terminal out of a value position.
//!
//! Closes ID(leaves): "Leaf trait for value positions, with impls for the syn terminals."
//!
//! A leaf is where the grammar stops. `MetaNameValue::value` is already an `Expr` and
//! `MetaList::tokens` can be read as `Punctuated<Expr, Comma>`, so both value positions funnel
//! through one trait and neither needs a parser of ours.
//!
//! NOTE(#leaves/local-trait): V[Tr(FromExpr).local && !Impl(TryFrom).for(syn)], "This is a trait and
//! not a TryFrom impl because of the orphan rule - VERIFIED E0117 for
//! `impl TryFrom<&Expr> for syn::LitStr`, a foreign trait on a foreign type. See
//! ID(vocab/orphan-shapes-the-api) for why newtyping the leaves to get TryFrom back was rejected:
//! it would put our wrapper in the author's own field types"

use syn::{Error, Expr, Ident, Path, Result, spanned::Spanned};

/// Read a terminal out of a value position.
pub trait FromExpr: Sized {
    fn from_expr(expr: &Expr) -> Result<Self>;
}

/// Read a node out of a `Meta` - the uniform field read.
///
/// NOTE(#leaves/uniform-field-read): V[Tr(FromMeta).impl(all)], "Every generated node implements
/// this, which is what lets meta_list! read a field WITHOUT knowing what shape it is: a flag reads
/// from Meta::Path, a leaf from a NameValue's expr, a nested list from Meta::List, and the caller
/// writes the same line for all three. There is deliberately no blanket
/// `impl<T: FromExpr> FromMeta for T` - it would conflict with the specific impls flag! and
/// meta_list! generate, so each macro emits its own one-liner instead"
pub trait FromMeta: Sized {
    fn from_meta(meta: &syn::Meta) -> Result<Self>;
}

/// The `FromMeta` body every leaf shares: a terminal is written as the right-hand side of `=`.
pub fn leaf_from_meta<T: FromExpr>(meta: &syn::Meta) -> Result<T> {
    match meta {
        syn::Meta::NameValue(nv) => T::from_expr(&nv.value),
        other => Err(Error::new_spanned(
            other,
            "expected `key = value` - this node is a value and has to be written as one",
        )),
    }
}

/// Implement [`FromExpr`] for the `Lit` family, which is uniform.
///
/// ```ignore
/// leaf! { LitStr = Str, LitInt = Int }
/// ```
///
/// Only the literals go through here. `Ident`, `Path` and `Expr` are each shaped differently and
/// are written out below - forcing them through one macro would cost more in macro machinery than
/// it saves in lines.
#[macro_export]
macro_rules! leaf {
    ( $( $ty:ident = $variant:ident ),+ $(,)? ) => {
        $(
            impl $crate::vocab::leaves::FromMeta for ::syn::$ty {
                fn from_meta(meta: &::syn::Meta) -> ::syn::Result<Self> {
                    $crate::vocab::leaves::leaf_from_meta::<::syn::$ty>(meta)
                }
            }

            impl $crate::vocab::leaves::FromExpr for ::syn::$ty {
                fn from_expr(expr: &::syn::Expr) -> ::syn::Result<Self> {
                    match expr {
                        ::syn::Expr::Lit(::syn::ExprLit {
                            lit: ::syn::Lit::$variant(value),
                            ..
                        }) => ::std::result::Result::Ok(value.clone()),
                        other => ::std::result::Result::Err(::syn::Error::new(
                            ::syn::spanned::Spanned::span(other),
                            ::std::concat!("expected a ", ::std::stringify!($variant), " literal"),
                        )),
                    }
                }
            }
        )+
    };
}

leaf! {
    LitStr = Str,
    LitInt = Int,
    LitFloat = Float,
    LitBool = Bool,
    LitChar = Char,
    LitByteStr = ByteStr,
}

// ---- the three that are not uniform ----------------------------------------

impl FromExpr for Ident {
    /// A bare ident arrives as a single-segment path expression - `Other(Blue)`'s payload is an
    /// `Expr::Path`, not an `Expr::Lit`.
    fn from_expr(expr: &Expr) -> Result<Self> {
        match expr {
            Expr::Path(path) => path
                .path
                .get_ident()
                .cloned()
                .ok_or_else(|| Error::new(expr.span(), "expected a bare identifier")),
            other => Err(Error::new(other.span(), "expected an identifier")),
        }
    }
}

impl FromExpr for Path {
    fn from_expr(expr: &Expr) -> Result<Self> {
        match expr {
            Expr::Path(path) => Ok(path.path.clone()),
            other => Err(Error::new(other.span(), "expected a path")),
        }
    }
}

/// The escape hatch: a field typed `Expr` accepts anything, which is what `guard = cfg!(..)` needs.
impl FromExpr for Expr {
    fn from_expr(expr: &Expr) -> Result<Self> {
        Ok(expr.clone())
    }
}

/// `bool` reads a literal here. Its OTHER reading - presence as a flag - is a different shape
/// entirely and belongs to `flag!`; ID(bool-double-duty) records that the two coexist deliberately.
impl FromExpr for bool {
    fn from_expr(expr: &Expr) -> Result<Self> {
        <syn::LitBool as FromExpr>::from_expr(expr).map(|lit| lit.value())
    }
}

macro_rules! leaf_meta {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl FromMeta for $ty {
                fn from_meta(meta: &syn::Meta) -> Result<Self> {
                    leaf_from_meta::<$ty>(meta)
                }
            }
        )+
    };
}

leaf_meta!(Ident, Path, Expr, bool);

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    fn expr(source: &str) -> Expr {
        parse_str(source).expect("an expression")
    }

    #[test]
    fn the_lit_family_reads_its_own_literal() {
        assert_eq!(
            syn::LitStr::from_expr(&expr(r#""thing""#)).unwrap().value(),
            "thing"
        );
        assert_eq!(
            syn::LitInt::from_expr(&expr("64")).unwrap().base10_parse::<u32>().unwrap(),
            64
        );
        assert!(bool::from_expr(&expr("true")).unwrap());
    }

    #[test]
    fn a_leaf_mismatch_names_what_was_expected() {
        let error = syn::LitStr::from_expr(&expr("64")).err().expect("not a string");
        assert_eq!(error.to_string(), "expected a Str literal");
        assert!(!error.to_compile_error().is_empty());
    }

    #[test]
    fn an_ident_is_a_path_expression_not_a_literal() {
        // `Other(Blue)` - the payload is Expr::Path, which is why Ident cannot go through leaf!
        assert_eq!(Ident::from_expr(&expr("Blue")).unwrap().to_string(), "Blue");
        assert!(Ident::from_expr(&expr("core::fmt::Debug")).is_err());
    }

    #[test]
    fn a_path_keeps_every_segment() {
        let path = Path::from_expr(&expr("::core::fmt::Debug")).unwrap();
        assert_eq!(path.segments.len(), 3);
        assert!(path.leading_colon.is_some());
    }

    #[test]
    fn a_free_form_expr_accepts_anything() {
        // `guard = cfg!(debug_assertions)` is impossible without this.
        assert!(Expr::from_expr(&expr("cfg!(debug_assertions)")).is_ok());
        assert!(Expr::from_expr(&expr("1 + 2")).is_ok());
    }
}
