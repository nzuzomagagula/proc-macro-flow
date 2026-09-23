// @review [ ]
//! The translation layer: `syn::Meta` in, grammar openings out.
//!
//! `syn::Meta` IS the attribute AST - `Path | List | NameValue` is exactly the set of shapes an
//! attribute can be written in, and `Punctuated<Meta, Comma>` is its spine. Nothing here invents a
//! node layer to sit beside it; this module only names the three places a grammar CONTINUES, and
//! translates a `Meta` into whichever one it is.
//!
//! NOTE(#openings): V[S(ListBody).T(TokenStream)] && V[S(ValueExpr).T(Expr)], "Two of the three
//! openings are UNPARSED, and that is the point rather than an omission. MetaList::tokens is a raw
//! TokenStream and MetaNameValue::value is an arbitrary Expr, so syn stops at exactly the depth
//! where a grammar's own meaning begins. An extractor carries these; a processor reads them. The
//! third, PathOnly, has nothing to read at all - presence is the signal."
//!
//! NOTE(#shape-is-the-opening): V[Tr(Shape).Ty(Input)], "A 'shape' is not a tag we attach to a
//! field, it is WHICH OPENING the payload arrives through. That collapses what looked like two
//! concepts into one: #[shape(AttributeKind::MetaList)] narrows a field to the ListBody opening,
//! and the narrowing is checked by rustc because the marker's Input type either matches what the
//! grammar type can read or does not."
//!
//! NOTE(#meta-vs-expr): V[F(metas) != F(exprs)], "The spine/leaf rule, made into two methods.
//! `colour(ColourSetting::Red)` reads its body as Metas because every element NAMES something;
//! `bounds(0, 64)` cannot, because a bare literal is not valid Meta at all. A grammar node picks
//! the reading its own shape implies - which is why Meta::List::tokens being raw is load-bearing
//! and not an inconvenience."

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::{parse::Parser, punctuated::Punctuated, Expr, Meta, Path, Token};

/// `#[flag]`, or a bare path in a value position. Nothing to parse.
#[derive(Clone, Copy)]
pub struct PathOnly<'ast>(pub &'ast Path);

/// `key(..)` - the body of a list, still in tokens.
#[derive(Clone, Copy)]
pub struct ListBody<'ast>(pub &'ast TokenStream);

/// `key = <expr>` - the right-hand side, already an `Expr` and needing no parser of ours.
#[derive(Clone, Copy)]
pub struct ValueExpr<'ast>(pub &'ast Expr);

impl<'ast> ListBody<'ast> {
    /// Read the body as a spine: every element names something.
    ///
    /// The comma case, which is every grammar that has not asked for otherwise.
    pub fn metas(self) -> syn::Result<Punctuated<Meta, Token![,]>> {
        self.separated::<Token![,]>()
    }

    /// Read the body as a spine with a separator of the caller's choosing.
    ///
    /// NOTE(#list-body/separator-is-a-type): V[M(separated).P(S)], "The separator is a TYPE
    /// PARAMETER, not an argument and not a third reading, which is ID(from/arity-from-type)'s rule
    /// applied to punctuation: what the grammar accepts is written in the type, so nothing can
    /// contradict it and no runtime value has to be threaded to the parse.
    ///
    /// NOTE(#separator): a custom separator is viable - VERIFIED that rustc accepts `#[attr(a; b)]`
    /// as an inert derive helper, parses it as Meta::List, and hands the tokens over verbatim.
    pub fn separated<S>(self) -> syn::Result<Punctuated<Meta, S>>
    where
        S: syn::parse::Parse,
    {
        Punctuated::<Meta, S>::parse_terminated.parse2(self.0.clone())
    }

    /// Read the body as leaves: bare values, which `Meta` cannot represent.
    ///
    /// Unchanged by the separator work: this is the LEAF reading, not a punctuation choice.
    /// `bounds(0, 64)` cannot go through a spine at all, because a bare literal is not valid
    /// `Meta` - which is the whole reason there are two readings and not one.
    pub fn exprs(self) -> syn::Result<Punctuated<Expr, Token![,]>> {
        Punctuated::<Expr, Token![,]>::parse_terminated.parse2(self.0.clone())
    }

    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }
}

/// Which opening a written `Meta` turned out to be.
#[derive(Clone, Copy)]
pub enum Opening<'ast> {
    Path(PathOnly<'ast>),
    List(ListBody<'ast>),
    Value(ValueExpr<'ast>),
}

impl<'ast> From<&'ast Meta> for Opening<'ast> {
    fn from(meta: &'ast Meta) -> Self {
        match meta {
            Meta::Path(path) => Opening::Path(PathOnly(path)),
            Meta::List(list) => Opening::List(ListBody(&list.tokens)),
            Meta::NameValue(nv) => Opening::Value(ValueExpr(&nv.value)),
        }
    }
}

crate::names! {
    /// The runtime identity of a shape - what a `Meta` turned out to be written as.
    ///
    /// `names!` and not `vocabulary!`, deliberately: a shape is SELECTED by a type path that rustc
    /// resolves (`#[shape(AttributeKind::MetaList)]`), never by an ident this crate compares. A
    /// `TryFrom<&Ident> for ShapeKind` would be a matching path that must never be used, which is
    /// ID(vocabulary/only-what-we-own) one level up. What it does need is a canonical spelling, so
    /// a WrongShape diagnostic reads "expected a `list`, found a `name-value`" from one declaration
    /// rather than from strings written out at each site.
    pub enum ShapeKind {
        Path = "path",
        List = "list",
        NameValue = "name-value",
    }
}

impl<'ast> Opening<'ast> {
    /// What the user actually wrote.
    pub fn kind(self) -> ShapeKind {
        match self {
            Opening::Path(_) => ShapeKind::Path,
            Opening::List(_) => ShapeKind::List,
            Opening::Value(_) => ShapeKind::NameValue,
        }
    }
}

impl From<&Meta> for ShapeKind {
    fn from(meta: &Meta) -> Self {
        Opening::from(meta).kind()
    }
}

mod sealed {
    pub trait Sealed {}
}

/// NOTE(#shape/two-facts): V[Ty(AttributeKind) != E(Opening)], "THE RULE the parallel vocabularies
/// exist to express, written down because the obvious reading of two structures with the same three
/// members is that one of them is redundant. They are not. They state DIFFERENT FACTS:
///
///   Ty(AttributeKind::MetaList)  - THIS TYPE DECLARES it can be read as a list. Compile time.
///                                  Known from the grammar author's own types, before any user
///                                  writes anything.
///   E(Opening)::List             - THE USER ACTUALLY WROTE a list. Runtime, and unavoidably so:
///                                  it is read off Ty(syn::Meta), which is input.
///
/// Collapsing them costs the diagnostic that matters most. 'expected a list, found a name-value'
/// needs BOTH halves at once, and a framework holding only one of them can say what it wanted or
/// what it got, never the pair. E(ReasonKind)::WrongShape is exactly that sentence.
///
/// This is the boundary NOTE(#type-backed) names: user input is runtime by definition, so the check
/// stays. What the rule demands is that its RESULT be typed - which it is, because C(KIND) is the
/// join and the comparison is over E(ShapeKind) rather than over spellings. Everything downstream
/// of the check is bound; only the check itself is not"
///
/// A shape, named by the opening its payload arrives through.
///
/// Sealed: there are three and exactly three, because `syn::Meta` has three variants. That is
/// Rust's real attribute grammar rather than a taxonomy of ours, which is also why it will not
/// drift as the language grows.
pub trait Shape: sealed::Sealed {
    type Input<'ast>;

    /// The runtime identity this marker selects.
    ///
    /// NOTE(#shape/bridge): V[Tr(Shape).C(KIND)], "Without this, the type-level markers and the
    /// runtime Opening were parallel structures with nothing joining them, and a WrongShape
    /// diagnostic had to hand-write both halves - the expected shape from the marker, the found
    /// shape from the Meta, with no compiler check that the two vocabularies agreed. Now a field's
    /// selected shape and what was written compare directly: `opening.kind() == S::KIND`"
    const KIND: ShapeKind;
}

/// The selector vocabulary, as a module of unit structs so `AttributeKind::MetaList` resolves as a
/// type path where the generator splices it.
///
/// Not an enum: a variant cannot be named in type position, and `#no-path-head` already established
/// that the path has to be an ARGUMENT to `#[shape(..)]` rather than the attribute head.
#[allow(non_snake_case)]
pub mod AttributeKind {
    pub struct MetaList;
    pub struct NamedValue;
    pub struct Path;
}

impl sealed::Sealed for AttributeKind::MetaList {}
impl sealed::Sealed for AttributeKind::NamedValue {}
impl sealed::Sealed for AttributeKind::Path {}

impl Shape for AttributeKind::MetaList {
    type Input<'ast> = ListBody<'ast>;
    const KIND: ShapeKind = ShapeKind::List;
}

impl Shape for AttributeKind::NamedValue {
    type Input<'ast> = ValueExpr<'ast>;
    const KIND: ShapeKind = ShapeKind::NameValue;
}

impl Shape for AttributeKind::Path {
    type Input<'ast> = PathOnly<'ast>;
    const KIND: ShapeKind = ShapeKind::Path;
}

// Every opening re-emits what it came from, which is what lets a selector be spliced verbatim and
// what gives `Error::new_spanned` a whole node to underline.
impl ToTokens for PathOnly<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
    }
}

impl ToTokens for ListBody<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
    }
}

impl ToTokens for ValueExpr<'_> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        self.0.to_tokens(tokens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{parse_str, Attribute, ItemStruct};

    fn attribute(source: &str) -> Attribute {
        let item: ItemStruct =
            parse_str(&format!("{source}\npub struct T;")).expect("the attribute parses");
        item.attrs.into_iter().next().expect("one attribute")
    }

    #[test]
    fn every_written_shape_translates_to_its_opening() {
        let list = attribute("#[configuration(colour(Red))]");
        assert!(matches!(Opening::from(&list.meta), Opening::List(_)));

        let path = attribute("#[no_clean]");
        assert!(matches!(Opening::from(&path.meta), Opening::Path(_)));

        let value = attribute(r#"#[name = "thing"]"#);
        assert!(matches!(Opening::from(&value.meta), Opening::Value(_)));
    }

    #[test]
    fn a_list_body_reads_as_a_spine_of_metas() {
        let attr = attribute(r#"#[configuration(colour(Red), name = "thing", no_clean)]"#);
        let Opening::List(body) = Opening::from(&attr.meta) else {
            panic!("a list");
        };

        let metas = body.metas().expect("every element names something");
        assert_eq!(metas.len(), 3);

        // and each element is itself an opening - this is the recursion
        let shapes: Vec<ShapeKind> = metas.iter().map(ShapeKind::from).collect();
        assert_eq!(
            shapes,
            [ShapeKind::List, ShapeKind::NameValue, ShapeKind::Path]
        );
    }

    #[test]
    fn a_list_body_of_bare_literals_reads_as_leaves() {
        // `bounds(0, 64)` is impossible without this: a bare literal is not valid Meta at all, so
        // reading the same body as a spine has to fail.
        let attr = attribute("#[bounds(0, 64)]");
        let Opening::List(body) = Opening::from(&attr.meta) else {
            panic!("a list");
        };

        assert!(body.metas().is_err(), "bare literals are not Meta");
        assert_eq!(body.exprs().expect("but they are Exprs").len(), 2);
    }

    #[test]
    fn an_empty_list_is_distinguishable() {
        // `targets()` - what NonEmpty has to reject.
        let attr = attribute("#[targets()]");
        let Opening::List(body) = Opening::from(&attr.meta) else {
            panic!("a list");
        };
        assert!(body.is_empty());
    }

    #[test]
    fn openings_re_emit_what_they_came_from() {
        // The property the compiler-checked selector depends on: tokens go back out unchanged.
        let attr = attribute("#[shape(AttributeKind::MetaList)]");
        let Opening::List(body) = Opening::from(&attr.meta) else {
            panic!("a list");
        };
        assert_eq!(
            body.to_token_stream().to_string(),
            "AttributeKind :: MetaList"
        );
    }

    #[test]
    fn a_selected_shape_and_a_written_one_compare_directly() {
        // THE WrongShape check, and there is no string on either side of it: the expected shape
        // comes off the marker type through Shape::KIND, the found shape off the written Meta.
        let written = attribute("#[colour(ColourSetting::Red)]");
        let found = ShapeKind::from(&written.meta);

        assert_eq!(found, <AttributeKind::MetaList as Shape>::KIND);
        assert_ne!(found, <AttributeKind::NamedValue as Shape>::KIND);

        // and the diagnostic reads its words from the same single declaration
        let expected = <AttributeKind::NamedValue as Shape>::KIND;
        assert_eq!(
            format!("expected a `{expected}`, found a `{found}`"),
            "expected a `name-value`, found a `list`"
        );
    }

    #[test]
    fn a_shapes_input_is_its_opening() {
        // Compile-time only: the marker's Input names the opening, which is what makes
        // #[shape(..)] checkable by rustc rather than by us.
        fn accepts<'ast, S: Shape>(_: S::Input<'ast>) {}

        let tokens = TokenStream::new();
        let expr: Expr = parse_str("1").unwrap();
        let path: Path = parse_str("Red").unwrap();

        accepts::<AttributeKind::MetaList>(ListBody(&tokens));
        accepts::<AttributeKind::NamedValue>(ValueExpr(&expr));
        accepts::<AttributeKind::Path>(PathOnly(&path));
    }

    #[test]
    fn a_body_can_be_read_with_a_custom_separator() {
        let item: syn::ItemStruct =
            syn::parse_str("#[t(a; b; c)]\npub struct T;").expect("the attribute parses");
        let attribute = item.attrs.first().expect("one attribute");
        let list = attribute.meta.require_list().expect("a list");
        let body = ListBody(&list.tokens);

        let metas = body
            .separated::<syn::Token![;]>()
            .expect("semicolons are a valid separator");
        assert_eq!(metas.len(), 3);

        // and the comma reading correctly REFUSES it - the separator is not a suggestion
        assert!(body.metas().is_err());
    }

    #[test]
    fn metas_is_the_comma_case_of_separated() {
        let item: syn::ItemStruct =
            syn::parse_str("#[t(a, b)]\npub struct T;").expect("the attribute parses");
        let list = item.attrs[0].meta.require_list().expect("a list");
        let body = ListBody(&list.tokens);

        assert_eq!(body.metas().unwrap().len(), body.separated::<syn::Token![,]>().unwrap().len());
    }
}
