// @review [ ]
//! `flag!` - a bare-path attribute, where presence is the whole signal.
//!
//! Closes ID(attribute/path): "Bare-path pattern `#[attr]` maps to a ZST/unit struct - no payload,
//! presence is the signal."
//!
//! NOTE(#flag/key-or-value): V[S(NoClean).accepts(\"no_clean\") && S(NoClean).accepts(\"NoClean\")],
//! "A flag accepts BOTH its key spelling and its type spelling, and that is not a case-folding rule
//! sneaking in - it is ID(resolve)'s key-or-value equivalence. A ZST value carries exactly the
//! information its key does, so `no_clean` and `NoClean` name the same node and there is nothing to
//! choose between them. Both are DECLARED, per ID(vocabulary/exact); deriving the second from the
//! first is ID(vocabulary/derive-spelling), which needs heck and therefore needs the derive"

/// Declare a flag: a unit struct matched from a bare path.
///
/// ```ignore
/// flag! {
///     /// `#[no_clean]` - presence is the signal.
///     pub struct NoClean = "no_clean" | "NoClean";
/// }
/// ```
///
/// Generates the unit struct plus `TryFrom<&syn::Path>`, `TryFrom<&syn::Meta>`, `ToTokens`,
/// `Default`, and the same `spelling` / `spellings` / `candidates` surface `names!` gives.
///
/// `TryFrom<&Meta>` rejects anything that is not a `Meta::Path`: a flag written as `no_clean(x)` or
/// `no_clean = 1` has a payload, and a flag by definition has nowhere to put one.
#[macro_export]
macro_rules! flag {
    (
        $(
            $(#[$meta:meta])*
            $vis:vis struct $name:ident = $spelling:literal $( | $alias:literal )* ;
        )+
    ) => {
        $(
            $(#[$meta])*
            #[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
            $vis struct $name;

            #[allow(dead_code)]
            impl $name {
                /// The canonical spelling. Aliases are accepted on the way in, never produced.
                pub const fn spelling() -> &'static str {
                    $spelling
                }

                /// Every accepted spelling, canonical first.
                pub const fn spellings() -> &'static [&'static str] {
                    &[$spelling $(, $alias)*]
                }

                pub fn candidates() -> ::std::string::String {
                    let names: ::std::vec::Vec<::std::string::String> = $name::spellings()
                        .iter()
                        .map(|spelling| ::std::format!("`{}`", spelling))
                        .collect();
                    names.join(", ")
                }

                fn expected(span: ::proc_macro2::Span) -> ::syn::Error {
                    ::syn::Error::new(
                        span,
                        ::std::format!("expected one of: {}", $name::candidates()),
                    )
                }
            }

            impl ::std::fmt::Display for $name {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    f.write_str($name::spelling())
                }
            }

            /// Re-emits the CANONICAL spelling, whichever one was written. A flag that round-tripped
            /// its alias would make the generated code depend on how the user spelled it.
            impl ::quote::ToTokens for $name {
                fn to_tokens(&self, tokens: &mut ::proc_macro2::TokenStream) {
                    ::quote::ToTokens::to_tokens(
                        &::proc_macro2::Ident::new(
                            $name::spelling(),
                            ::proc_macro2::Span::call_site(),
                        ),
                        tokens,
                    );
                }
            }

            impl ::std::convert::TryFrom<&::syn::Path> for $name {
                type Error = ::syn::Error;

                fn try_from(path: &::syn::Path) -> ::std::result::Result<Self, Self::Error> {
                    let matched = path
                        .get_ident()
                        .map(|ident| {
                            let written = ident.to_string();
                            $name::spellings().iter().any(|spelling| *spelling == written)
                        })
                        .unwrap_or(false);

                    if matched {
                        ::std::result::Result::Ok($name)
                    } else {
                        ::std::result::Result::Err($name::expected(
                            ::syn::spanned::Spanned::span(path),
                        ))
                    }
                }
            }

            impl $crate::vocab::leaves::FromMeta for $name {
                fn from_meta(meta: &::syn::Meta) -> ::syn::Result<Self> {
                    <$name as ::std::convert::TryFrom<&::syn::Meta>>::try_from(meta)
                }
            }

            impl ::std::convert::TryFrom<&::syn::Meta> for $name {
                type Error = ::syn::Error;

                /// Only a `Meta::Path` can be a flag - anything else carries a payload a flag has
                /// nowhere to put.
                fn try_from(meta: &::syn::Meta) -> ::std::result::Result<Self, Self::Error> {
                    match meta {
                        ::syn::Meta::Path(path) => {
                            <$name as ::std::convert::TryFrom<&::syn::Path>>::try_from(path)
                        }
                        other => ::std::result::Result::Err(::syn::Error::new(
                            ::syn::spanned::Spanned::span(other),
                            ::std::format!(
                                "`{}` takes no arguments - it is a flag, and its presence is the \
                                 whole signal",
                                $name::spelling()
                            ),
                        )),
                    }
                }
            }
        )+
    };
}

#[cfg(test)]
mod tests {
    use syn::{parse_str, Attribute, ItemStruct, Meta, Path};

    flag! {
        /// `#[no_clean]`
        pub struct NoClean = "no_clean" | "NoClean";

        /// A second one, to prove the macro takes a list.
        pub struct Verbose = "verbose";
    }

    fn meta_of(source: &str) -> Meta {
        let item: ItemStruct =
            parse_str(&format!("{source}\npub struct T;")).expect("the attribute parses");
        let attr: Attribute = item.attrs.into_iter().next().expect("one attribute");
        attr.meta
    }

    #[test]
    fn a_flag_accepts_its_key_and_its_value_spelling() {
        // ID(flag/key-or-value): a ZST value carries what its key does, so both name the same node.
        assert!(NoClean::try_from(&meta_of("#[no_clean]")).is_ok());
        assert!(NoClean::try_from(&meta_of("#[NoClean]")).is_ok());
    }

    #[test]
    fn a_flag_re_emits_the_canonical_spelling() {
        use quote::ToTokens;

        // Written as the value, emitted as the key: generated code must not depend on which
        // spelling the user picked.
        let flag = NoClean::try_from(&meta_of("#[NoClean]")).unwrap();
        assert_eq!(flag.to_token_stream().to_string(), "no_clean");
    }

    #[test]
    fn a_flag_with_a_payload_is_rejected_and_says_why() {
        let error = NoClean::try_from(&meta_of("#[no_clean(extra)]"))
            .err()
            .expect("has a payload");
        assert!(error.to_string().contains("takes no arguments"), "{error}");

        let error = NoClean::try_from(&meta_of("#[no_clean = 1]"))
            .err()
            .expect("has a payload");
        assert!(error.to_string().contains("takes no arguments"), "{error}");
    }

    #[test]
    fn an_unknown_name_carries_the_candidate_list() {
        let error = NoClean::try_from(&meta_of("#[no_cleen]"))
            .err()
            .expect("not this flag");
        assert_eq!(error.to_string(), "expected one of: `no_clean`, `NoClean`");
    }

    #[test]
    fn matching_is_exact_and_single_segment() {
        let shouting: Path = parse_str("NO_CLEAN").unwrap();
        assert!(NoClean::try_from(&shouting).is_err());

        // these are names we own and never qualify
        let qualified: Path = parse_str("foo::no_clean").unwrap();
        assert!(NoClean::try_from(&qualified).is_err());
    }

    #[test]
    fn flags_are_distinct_types_not_a_shared_enum() {
        // The point of a ZST per flag: `Verbose` cannot be passed where `NoClean` is wanted, so a
        // field typed on one cannot silently accept the other.
        assert!(Verbose::try_from(&meta_of("#[verbose]")).is_ok());
        assert!(NoClean::try_from(&meta_of("#[verbose]")).is_err());
    }
}
