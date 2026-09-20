// @review [ ]
//! `names!` and `vocabulary!` - closed sets of names, and the ones that are matched from tokens.

/// Declare a closed set of names with canonical spellings.
///
/// Generates the enum plus `ALL`, `spelling`, `spellings`, `candidates`, `from_spelling`,
/// `Display` and `From<Self> for &'static str`. It does NOT generate any matching from tokens -
/// that is `vocabulary!`, which builds on this.
///
/// Reach for `names!` when a set needs one canonical spelling for diagnostics but is never read
/// back out of what the user wrote. `ShapeKind` is the example: a shape is selected by a TYPE path
/// that rustc resolves, never by an ident we compare, so a `TryFrom<&Ident>` for it would be a
/// matching path that must never be used.
///
/// ```ignore
/// names! {
///     pub enum ShapeKind {
///         Path = "path",
///         List = "list",
///         NameValue = "name-value",
///     }
/// }
/// ```
#[macro_export]
macro_rules! names {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident = $spelling:literal $( | $alias:literal )*
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                $variant
            ),+
        }

        #[allow(dead_code)]
        impl $name {
            /// Every name in the set, in declaration order.
            pub const ALL: &'static [$name] = &[ $( $name::$variant ),+ ];

            /// The canonical spelling. Aliases are accepted on the way in and never produced.
            pub const fn spelling(self) -> &'static str {
                match self {
                    $( $name::$variant => $spelling ),+
                }
            }

            /// Every accepted spelling for this one name, canonical first.
            pub const fn spellings(self) -> &'static [&'static str] {
                match self {
                    $( $name::$variant => &[$spelling $(, $alias)*] ),+
                }
            }

            /// The candidate list a diagnostic shows, in `lookahead1`'s shape.
            pub fn candidates() -> ::std::string::String {
                let names: ::std::vec::Vec<::std::string::String> = $name::ALL
                    .iter()
                    .map(|entry| ::std::format!("`{}`", entry.spelling()))
                    .collect();
                names.join(", ")
            }

            /// Match a bare spelling. Exact - see ID(vocabulary/exact).
            pub fn from_spelling(text: &str) -> ::std::option::Option<$name> {
                match text {
                    $( $spelling $( | $alias )* => ::std::option::Option::Some($name::$variant), )+
                    _ => ::std::option::Option::None,
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.spelling())
            }
        }

        impl ::std::convert::From<$name> for &'static str {
            fn from(entry: $name) -> &'static str {
                entry.spelling()
            }
        }
    };
}

/// A `names!` set that is additionally MATCHED from written tokens.
///
/// Adds `TryFrom<&syn::Ident>` and `TryFrom<&syn::Path>`. Use this only where the framework owns
/// the name AND reads it out of the user's tokens - see ID(vocabulary/only-what-we-own). A set that
/// merely needs a canonical spelling for diagnostics wants `names!` instead: generating a matching
/// path that must never be used is its own trap.
///
/// ```ignore
/// vocabulary! {
///     /// Helper attributes this derive registers.
///     pub enum SyntaxHelper {
///         Shape = "shape",
///         Alias = "alias" | "renamed",   // extra spellings are explicit
///     }
/// }
/// ```
///
/// A spelling cannot be a Rust keyword: `syn::Ident` refuses to parse one, so `"as"` or `"type"`
/// would never match anything that reached us. Nothing enforces that - it simply never fires.
#[macro_export]
macro_rules! vocabulary {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident { $($body:tt)* }
    ) => {
        $crate::names! {
            $(#[$meta])*
            $vis enum $name { $($body)* }
        }

        impl ::std::convert::TryFrom<&::syn::Ident> for $name {
            type Error = ::syn::Error;

            fn try_from(ident: &::syn::Ident) -> ::std::result::Result<Self, Self::Error> {
                match $name::from_spelling(&ident.to_string()) {
                    ::std::option::Option::Some(entry) => ::std::result::Result::Ok(entry),
                    ::std::option::Option::None => ::std::result::Result::Err(::syn::Error::new(
                        ::syn::spanned::Spanned::span(ident),
                        ::std::format!("expected one of: {}", $name::candidates()),
                    )),
                }
            }
        }

        impl ::std::convert::TryFrom<&::syn::Path> for $name {
            type Error = ::syn::Error;

            /// A vocabulary name is always a single segment: these are names we own, and we never
            /// invent a module to qualify them with.
            fn try_from(path: &::syn::Path) -> ::std::result::Result<Self, Self::Error> {
                match path.get_ident() {
                    ::std::option::Option::Some(ident) => {
                        <$name as ::std::convert::TryFrom<&::syn::Ident>>::try_from(ident)
                    }
                    ::std::option::Option::None => ::std::result::Result::Err(::syn::Error::new(
                        ::syn::spanned::Spanned::span(path),
                        ::std::format!("expected one of: {}", $name::candidates()),
                    )),
                }
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use syn::{parse_str, Ident, Path};

    vocabulary! {
        /// The helper attributes the syntax stage registers.
        pub enum Helper {
            Shape = "shape",
            Alias = "alias" | "renamed",
        }
    }

    #[test]
    fn a_name_round_trips_through_its_spelling() {
        assert_eq!(Helper::Shape.spelling(), "shape");
        assert_eq!(<&str>::from(Helper::Alias), "alias");
        assert_eq!(Helper::Alias.to_string(), "alias");
    }

    #[test]
    fn try_into_is_the_native_comparison() {
        let ident: Ident = parse_str("shape").unwrap();
        let helper: Helper = (&ident).try_into().expect("a known name");
        assert_eq!(helper, Helper::Shape);

        // and from a Path, which is what an attribute head actually is
        let path: Path = parse_str("alias").unwrap();
        assert_eq!(Helper::try_from(&path).unwrap(), Helper::Alias);
    }

    #[test]
    fn an_alias_is_accepted_but_never_produced() {
        let ident: Ident = parse_str("renamed").unwrap();
        assert_eq!(Helper::try_from(&ident).unwrap(), Helper::Alias);

        // canonical spelling comes back out, not the alias that was written
        assert_eq!(Helper::try_from(&ident).unwrap().spelling(), "alias");
        assert_eq!(Helper::Alias.spellings(), &["alias", "renamed"]);
    }

    #[test]
    fn an_unknown_name_carries_the_candidate_list() {
        let ident: Ident = parse_str("shpae").unwrap();
        let error = Helper::try_from(&ident).expect_err("not a known name");

        // Same shape lookahead1 produces for syn's own keywords, and generated from the SAME
        // declaration the match arms come from - so the list cannot drift from what is accepted.
        assert_eq!(error.to_string(), "expected one of: `shape`, `alias`");
        assert!(!error.to_compile_error().is_empty());
    }

    #[test]
    fn matching_is_exact() {
        // No case folding: ID(vocabulary/exact). `Shape` is not `shape`.
        let shouting: Ident = parse_str("Shape").unwrap();
        assert!(Helper::try_from(&shouting).is_err());
    }

    #[test]
    fn a_multi_segment_path_is_not_a_vocabulary_name() {
        // These are names we own and never qualify, so `foo::shape` is not `shape`.
        let qualified: Path = parse_str("foo::shape").unwrap();
        assert!(Helper::try_from(&qualified).is_err());
    }

    #[test]
    fn the_set_is_enumerable_for_diagnostics() {
        assert_eq!(Helper::ALL, &[Helper::Shape, Helper::Alias]);
        assert_eq!(Helper::candidates(), "`shape`, `alias`");
    }
}
