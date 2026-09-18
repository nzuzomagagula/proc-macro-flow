// @review [ ]
//! `name_value!` - a newtype over a leaf, read from the right-hand side of `key = value`.
//!
//! Closes the struct half of ID(attribute/name-value): "Name-value pattern maps to a struct."
//!
//! NOTE(#name-value/no-key): V[M(name_value).!takes(key)], "A name-value node does NOT own the key
//! it is written under. `name = \"thing\"` puts `name` on the FIELD of the containing struct, not on
//! ConfigName - which is what lets one newtype sit behind two differently-named fields, and what
//! `#[alias(label)]` adjusts. This is the asymmetry ID(syntax/names) already states: KEYS ARE
//! IDENTS, VALUES ARE PATHS. A key belongs to the structure; a value belongs to the type"
//!
//! NOTE(#name-value/why-newtype): V[S(ConfigName).T(LitStr)], "The wrapper is not ceremony. A bare
//! `LitStr` field says only 'a string goes here'; ConfigName says WHICH string, so two fields of
//! the same underlying terminal stay distinct types and cannot be swapped. It is also the hook for
//! a validating constructor later - the place ID(pipeline/validity-scope)'s surface checks would
//! attach, since 'is this value within a bound' is exactly a newtype's business"

/// Declare a newtype over a leaf.
///
/// ```ignore
/// name_value! {
///     /// `name = "thing"`
///     pub struct ConfigName = LitStr;
/// }
/// ```
///
/// Generates the newtype plus `FromExpr` delegating to the inner leaf, `TryFrom<&syn::Expr>`,
/// `TryFrom<&syn::MetaNameValue>`, `Deref`, and `ToTokens`.
///
/// `TryFrom<&MetaNameValue>` and not `TryFrom<&Meta>`: which key this sits under is the containing
/// struct's business, so this reads a name-value it has already been handed. See
/// ID(name-value/no-key).
#[macro_export]
macro_rules! name_value {
    (
        $(
            $(#[$meta:meta])*
            $vis:vis struct $name:ident = $leaf:ty ;
        )+
    ) => {
        $(
            $(#[$meta])*
            // No `Debug`: syn's types only implement it under the `extra-traits` feature, which is
            // a real compile-time cost to force on every downstream user for a derive they may not
            // want. An author whose leaf supports it can add `#[derive(Debug)]` above the
            // declaration - the outer attributes are forwarded.
            #[derive(Clone)]
            $vis struct $name(pub $leaf);

            #[allow(dead_code)]
            impl $name {
                pub fn into_inner(self) -> $leaf {
                    self.0
                }
            }

            impl $crate::vocab::leaves::FromExpr for $name {
                fn from_expr(expr: &::syn::Expr) -> ::syn::Result<Self> {
                    <$leaf as $crate::vocab::leaves::FromExpr>::from_expr(expr).map($name)
                }
            }

            impl $crate::vocab::leaves::FromMeta for $name {
                fn from_meta(meta: &::syn::Meta) -> ::syn::Result<Self> {
                    $crate::vocab::leaves::leaf_from_meta::<$name>(meta)
                }
            }

            impl ::std::convert::TryFrom<&::syn::Expr> for $name {
                type Error = ::syn::Error;

                fn try_from(expr: &::syn::Expr) -> ::std::result::Result<Self, Self::Error> {
                    <$name as $crate::vocab::leaves::FromExpr>::from_expr(expr)
                }
            }

            impl ::std::convert::TryFrom<&::syn::MetaNameValue> for $name {
                type Error = ::syn::Error;

                fn try_from(
                    nv: &::syn::MetaNameValue,
                ) -> ::std::result::Result<Self, Self::Error> {
                    <$name as $crate::vocab::leaves::FromExpr>::from_expr(&nv.value)
                }
            }

            impl ::std::ops::Deref for $name {
                type Target = $leaf;

                fn deref(&self) -> &$leaf {
                    &self.0
                }
            }

            impl ::quote::ToTokens for $name {
                fn to_tokens(&self, tokens: &mut ::proc_macro2::TokenStream) {
                    ::quote::ToTokens::to_tokens(&self.0, tokens);
                }
            }
        )+
    };
}

#[cfg(test)]
mod tests {
    use crate::vocab::leaves::FromExpr;
    use syn::{Attribute, ItemStruct, LitInt, LitStr, Meta, MetaNameValue, parse_str};

    name_value! {
        /// `name = "thing"`
        pub struct ConfigName = LitStr;

        /// A second over the SAME leaf, to prove they stay distinct types.
        pub struct Label = LitStr;

        pub struct Retries = LitInt;
    }

    fn name_value_of(source: &str) -> MetaNameValue {
        let item: ItemStruct =
            parse_str(&format!("{source}\npub struct T;")).expect("the attribute parses");
        let attr: Attribute = item.attrs.into_iter().next().expect("one attribute");
        match attr.meta {
            Meta::NameValue(nv) => nv,
            _ => panic!("expected a name-value attribute"),
        }
    }

    #[test]
    fn a_newtype_reads_the_right_hand_side() {
        let name = ConfigName::try_from(&name_value_of(r#"#[name = "thing"]"#)).unwrap();
        assert_eq!(name.value(), "thing"); // through Deref
        assert_eq!(name.into_inner().value(), "thing");
    }

    #[test]
    fn the_key_is_not_the_newtypes_business() {
        // ID(name-value/no-key): the SAME type reads regardless of the key it sat under, because
        // the key belongs to the field in the containing struct.
        assert!(ConfigName::try_from(&name_value_of(r#"#[name = "thing"]"#)).is_ok());
        assert!(ConfigName::try_from(&name_value_of(r#"#[label = "thing"]"#)).is_ok());
        assert!(ConfigName::try_from(&name_value_of(r#"#[anything = "thing"]"#)).is_ok());
    }

    #[test]
    fn two_newtypes_over_one_leaf_stay_distinct() {
        // The reason the wrapper is not ceremony: these cannot be swapped, though both are LitStr.
        fn takes_a_name(_: ConfigName) {}

        let name = ConfigName::try_from(&name_value_of(r#"#[name = "a"]"#)).unwrap();
        takes_a_name(name);

        let label = Label::try_from(&name_value_of(r#"#[label = "b"]"#)).unwrap();
        assert_eq!(label.value(), "b");
        // takes_a_name(label) would not compile - different types over the same leaf.
    }

    #[test]
    fn a_wrong_leaf_is_rejected_with_the_leafs_own_message() {
        let error = ConfigName::try_from(&name_value_of("#[name = 64]")).err().expect("not a string");
        assert_eq!(error.to_string(), "expected a Str literal");

        let error = Retries::try_from(&name_value_of(r#"#[retries = "x"]"#)).err().expect("not an int");
        assert_eq!(error.to_string(), "expected a Int literal");
    }

    #[test]
    fn a_newtype_is_itself_a_leaf() {
        // So it composes: a newtype can sit wherever a terminal can, including inside Option<T>.
        let expr: syn::Expr = parse_str(r#""thing""#).unwrap();
        assert!(ConfigName::from_expr(&expr).is_ok());
    }

    #[test]
    fn a_newtype_re_emits_its_inner_value() {
        use quote::ToTokens;

        let name = ConfigName::try_from(&name_value_of(r#"#[name = "thing"]"#)).unwrap();
        assert_eq!(name.to_token_stream().to_string(), r#""thing""#);
    }
}
