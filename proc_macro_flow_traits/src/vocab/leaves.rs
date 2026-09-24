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

use syn::{spanned::Spanned, Error, Expr, Ident, Path, Result};

/// Read a terminal out of a value position.
pub trait FromExpr: Sized {
    fn from_expr(expr: &Expr) -> Result<Self>;

    /// The `FromMeta` body every leaf shares: a terminal is written as the right-hand side of `=`.
    ///
    /// Provided here rather than as a free function (NOTE(#pipeline/no-free-functions)), so a leaf
    /// spells its `FromMeta` impl `Self::leaf_from_meta(meta)` and names nothing twice.
    fn leaf_from_meta(meta: &syn::Meta) -> Result<Self> {
        match meta {
            syn::Meta::NameValue(nv) => Self::from_expr(&nv.value),
            other => Err(Error::new_spanned(
                other,
                "expected `key = value` - this node is a value and has to be written as one",
            )),
        }
    }
}

/// Read a node out of a `Meta` - the uniform field read.
///
/// NOTE(#leaves/uniform-field-read): V[Tr(FromMeta).impl(all)], "Every generated node implements
/// this, which is what lets meta_list! read a field WITHOUT knowing what shape it is: a flag reads
/// from Meta::Path, a leaf from a NameValue's expr, a nested list from Meta::List, and the caller
/// writes the same line for all three. There is deliberately no blanket
/// `impl<T: FromExpr> FromMeta for T` - it would conflict with the specific impls flag! and
/// meta_list! generate, so each macro emits its own one-liner instead"
/// NOTE(#forwarding/no-option): V[!Impl(Option<T>).impl(FromMeta)], "There is DELIBERATELY no
/// `impl<T: FromMeta> FromMeta for Option<T>`, and its absence is load-bearing rather than an
/// oversight. ID(forwarding) asks for adapters over Option, Vec and Box; Vec and Box are fine and
/// Option must never be written.
///
/// WHY. Arity is read off the field's TYPE (ID(from/arity-from-type)), and M(meta_list) reads it
/// SYNTACTICALLY - it matches the tokens `Option < .. >` before it matches a bare type, because
/// macro_rules cannot inspect a captured `$ty:ty`. So a field spelled `std::option::Option<LitStr>`
/// is treated as REQUIRED, and the same is true of `type Maybe<T> = Option<T>;`.
///
/// VERIFIED that the mistake is currently LOUD: such a field fails to compile with
/// `the trait bound Option<LitStr>: FromMeta is not satisfied`, because the macro then generates a
/// required read of a type that has no reading. Adding the blanket impl would satisfy that bound
/// and the field would silently become required instead - a wrong meaning rather than an error.
///
/// So the missing impl is what keeps a mis-read arity a compile error at the AUTHOR's site. When
/// ID(syntax/derive) lands the derive will PARSE the type and get this right for real, because a
/// proc macro can look at `segments.last()` - which is exactly what F(unwrap_generic) in the derive
/// crate already does. Revisit then, not before"
pub trait FromMeta: Sized {
    fn from_meta(meta: &syn::Meta) -> Result<Self>;
}

/// A grammar node read from the INSIDE of its delimiters, with no head in front of it.
///
/// NOTE(#from-body/one-reader-two-entries): V[Tr(FromMeta).delegates(Tr(FromBody))], "This is
/// ID(entry)'s 'from_body does the work; the other two are thin adapters', built. The whole reason
/// it is cheap is that the reader never wanted the head: `from_meta`'s first act is
/// `meta.require_list()?` purely to reach `list.tokens`, and it does not look at `meta.path()` at
/// all. So a helper attribute's body and an ATTRIBUTE MACRO'S ARGUMENTS are already the same
/// thing: rustc hands a proc_macro_attribute its arguments ALREADY UNWRAPPED, which is exactly
/// the token stream `require_list` was digging for.
///
/// What this buys is that an attribute macro's arguments obey the SAME RULES as a helper
/// attribute's: the same keys, the same aliases, the same arity read off the field type, the same
/// shape bounds, the same did-you-mean from Ty(Node), the same accumulation. Not a second grammar
/// that has to be kept in step with the first - the same one, entered a different way"
// NOTE(#assert/leaves-are-askable): V[M(leaf).emits(Impl(Assert))], "Every leaf gets an EMPTY
// Tr(Assert), and the emptiness is not the point - the EXISTENCE is. A grammar's generated
// `assert` descends into each of its fields so that a nested grammar's rules are reached, and a
// descent needs every field type to be askable, leaves included. Without these the derive could
// only descend into fields it could prove were grammars, which it cannot do from a type alone.
//
// They are emitted from M(leaf) and M(leaf_meta) - the same two lists that already decide what a
// leaf is - rather than written out again, so a leaf added later cannot be askable in one sense
// and not the other."
// TODO[x](#attribute/args-are-a-grammar): C[Tr(FromBody)] && V[Tr(FromMeta).delegates(Tr(FromBody))],
// "An attribute macro's arguments must obey the SAME rules as a helper attribute - same keys,
// aliases, arity, shapes, did-you-mean - rather than a second grammar kept in step by hand. The
// reader never wanted the head, so one split is the whole adapter"
pub trait FromBody: Sized {
    /// Read the body.
    ///
    /// NOTE(#from-body/fallback-is-tokens-not-a-span): V[F(from_body).A(at).T(ToTokens)], "`at` is
    /// what a complaint falls back to when it has no token of its own to point at, and it is
    /// `&impl ToTokens` rather than a Ty(Span) for a reason that would otherwise be discovered as a
    /// regression. The missing-key arm uses `Error::new_spanned(..)`, which underlines a node's
    /// whole start..end range; a bare Ty(Span) collapses that to the FIRST TOKEN, because
    /// `Span::join` is nightly-only - the same constraint ID(reason/span-not-node) records. So
    /// F(from_meta) passes `meta` and the derive path's spans are byte-identical to what they were
    /// before this trait existed.
    ///
    /// What it buys is the asymmetry ID(entry) flagged and could not otherwise handle: EMPTY
    /// ARGUMENTS HAVE NO SPAN. `#[trace]` and `#[trace()]` are indistinguishable to an attribute
    /// macro, so a required key missing from a bare `#[trace]` has nothing at all to underline.
    /// The entry passes the ANNOTATED ITEM, and the complaint lands on the function the attribute
    /// was written on instead of nowhere"
    fn from_body<S>(body: &proc_macro2::TokenStream, at: &S) -> Result<Self>
    where
        S: quote::ToTokens + ?Sized;
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
                    <::syn::$ty as $crate::vocab::leaves::FromExpr>::leaf_from_meta(meta)
                }
            }

            // A leaf states no rules, but it must be ASKABLE, or a grammar holding one cannot
            // descend into its fields at all. Emitted from the same list that already decides what
            // a leaf IS, so there is no second set of names to keep in step -
            // NOTE(#assert/leaves-are-askable).
            impl $crate::assert::Assert for ::syn::$ty {}

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
                    <$ty as FromExpr>::leaf_from_meta(meta)
                }
            }

            impl crate::assert::Assert for $ty {}
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
            syn::LitInt::from_expr(&expr("64"))
                .unwrap()
                .base10_parse::<u32>()
                .unwrap(),
            64
        );
        assert!(bool::from_expr(&expr("true")).unwrap());
    }

    #[test]
    fn a_leaf_mismatch_names_what_was_expected() {
        let error = syn::LitStr::from_expr(&expr("64"))
            .err()
            .expect("not a string");
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
