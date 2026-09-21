// @review [ ]
//! `variants!` - an enum read from a VALUE position.
//!
//! Closes ID(attribute/list): "Macro-list pattern maps to an enum - one variant per allowed
//! ident/path in the list."
//!
//! Three written forms, one per variant shape:
//!
//! ```text
//! ColourSetting::Red            Meta::Path        unit variant
//! Other(Blue)                   Meta::List        tuple variant, positional Exprs
//! Rgb(r = 12, g = 34, b = 56)   Meta::List        struct variant, named Metas
//! ```
//!
//! NOTE(#variants/matched-not-resolved): V[M(variants).matches(last_segment)], "This is the one
//! place ID(vocab/match-or-splice) bites, and it bites in the direction the rule predicts: the
//! caller needs a VALUE - an actual ColourSetting to branch on - and rustc cannot hand us one, only
//! check one. So variant names are MATCHED, on the last path segment, and the documented cost is
//! that a renamed import is invisible: `use ColourSetting::Other as O;` then `colour(O(Blue))` will
//! not be seen. `ColourSetting::Red` and `Red` ARE the same node, because suffix matching handles
//! qualification. Contrast #[shape(..)], where no value is needed and the tokens are spliced for
//! rustc to resolve - which is why a renamed import DOES work there"
//!
//! NOTE(#variants/payload-is-pre-formed): V[M(variants).acc(decl)], "The muncher accumulates each
//! variant's payload as already-formed TOKENS rather than as structured data. It has to: a macro
//! cannot expand into a partial enum body, so `enum E { $crate::variants!(@decl ..) }` is not
//! legal. Accumulating the exact declaration tokens and splicing them with one repetition is the
//! way around it, and is why the three shapes converge on a single `$($decl)*` at emission"

/// Declare an enum read from a value position.
///
/// ```ignore
/// variants! {
///     pub enum ColourSetting {
///         Red = "Red",
///         Other = "Other" (Ident),
///         Rgb = "Rgb" { r: LitInt, g: LitInt, b: LitInt },
///     }
/// }
/// ```
///
/// Generates the enum plus `FromMeta`, `FromExpr` (unit variants only - a bare `Red` in a value
/// position is a path expression) and `TryFrom<&syn::Meta>`.
#[macro_export]
macro_rules! variants {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $($body:tt)*
        }
    ) => {
        $crate::variants!(@munch
            head { $(#[$meta])* $vis $name }
            done { }
            rest { $($body)* }
        );
    };

    // struct variant - must precede the tuple and unit rules
    (@munch
        head { $($head:tt)* }
        done { $($done:tt)* }
        rest { $v:ident = $key:literal { $($f:ident : $t:ty),+ $(,)? } , $($rest:tt)* }
    ) => {
        $crate::variants!(@munch
            head { $($head)* }
            done { $($done)* [ decl { $v { $($f: $t),+ } } key { $key } named { $($f: $t),+ } ] }
            rest { $($rest)* }
        );
    };

    // tuple variant
    (@munch
        head { $($head:tt)* }
        done { $($done:tt)* }
        rest { $v:ident = $key:literal ( $($t:ty),+ $(,)? ) , $($rest:tt)* }
    ) => {
        $crate::variants!(@munch
            head { $($head)* }
            done { $($done)* [ decl { $v ( $($t),+ ) } key { $key } positional { $($t),+ } ] }
            rest { $($rest)* }
        );
    };

    // unit variant
    (@munch
        head { $($head:tt)* }
        done { $($done:tt)* }
        rest { $v:ident = $key:literal , $($rest:tt)* }
    ) => {
        $crate::variants!(@munch
            head { $($head)* }
            done { $($done)* [ decl { $v } key { $key } unit { } ] }
            rest { $($rest)* }
        );
    };

    (@munch
        head { $(#[$meta:meta])* $vis:vis $name:ident }
        done { $([ decl { $($decl:tt)* } key { $key:literal } $kind:ident { $($data:tt)* } ])* }
        rest { }
    ) => {
        $(#[$meta])*
        #[derive(Clone)]
        $vis enum $name {
            $( $($decl)* ),*
        }

        #[allow(dead_code)]
        impl $name {
            /// Every variant name this node accepts, in declaration order.
            pub const NAMES: &'static [&'static str] = &[ $($key),* ];

            fn unknown(span: ::proc_macro2::Span) -> ::syn::Error {
                let quoted: ::std::vec::Vec<::std::string::String> = $name::NAMES
                    .iter()
                    .map(|name| ::std::format!("`{}`", name))
                    .collect();
                ::syn::Error::new(
                    span,
                    ::std::format!("expected one of: {}", quoted.join(", ")),
                )
            }
        }

        impl $crate::vocab::leaves::FromMeta for $name {
            fn from_meta(meta: &::syn::Meta) -> ::syn::Result<Self> {
                <$name as ::std::convert::TryFrom<&::syn::Meta>>::try_from(meta)
            }
        }

        /// A unit variant written bare in a value position - `colour(Red)` parses its body as
        /// `Expr`s, so `Red` arrives as a path expression rather than as a `Meta`.
        impl $crate::vocab::leaves::FromExpr for $name {
            fn from_expr(expr: &::syn::Expr) -> ::syn::Result<Self> {
                let path = <::syn::Path as $crate::vocab::leaves::FromExpr>::from_expr(expr)?;
                let span = ::syn::spanned::Spanned::span(&path);
                let written = $crate::vocab::walk::Named::last_segment(&path)
                    .ok_or_else(|| $name::unknown(span))?;

                match written.as_str() {
                    $(
                        $key => $crate::variants!(@from_path $kind $name $($decl)*),
                    )*
                    _ => ::std::result::Result::Err($name::unknown(span)),
                }
            }
        }

        impl ::std::convert::TryFrom<&::syn::Meta> for $name {
            type Error = ::syn::Error;

            fn try_from(meta: &::syn::Meta) -> ::std::result::Result<Self, Self::Error> {
                match meta {
                    ::syn::Meta::Path(path) => {
                        let span = ::syn::spanned::Spanned::span(path);
                        let written = $crate::vocab::walk::Named::last_segment(path)
                            .ok_or_else(|| $name::unknown(span))?;

                        match written.as_str() {
                            $(
                                $key => $crate::variants!(@from_path $kind $name $($decl)*),
                            )*
                            _ => ::std::result::Result::Err($name::unknown(span)),
                        }
                    }
                    ::syn::Meta::List(list) => {
                        let span = ::syn::spanned::Spanned::span(&list.path);
                        let written = $crate::vocab::walk::Named::last_segment(&list.path)
                            .ok_or_else(|| $name::unknown(span))?;
                        let body = $crate::meta::ListBody(&list.tokens);

                        match written.as_str() {
                            $(
                                $key => $crate::variants!(
                                    @from_list $kind $name $key body span { $($data)* } $($decl)*
                                ),
                            )*
                            _ => ::std::result::Result::Err($name::unknown(span)),
                        }
                    }
                    other => ::std::result::Result::Err(::syn::Error::new_spanned(
                        other,
                        ::std::concat!(
                            ::std::stringify!($name),
                            " has no `key = value` reading"
                        ),
                    )),
                }
            }
        }
    };

    // --- reading a variant written as a bare path ---------------------------
    (@from_path unit $name:ident $v:ident) => {
        ::std::result::Result::Ok($name::$v)
    };
    (@from_path positional $name:ident $v:ident $($rest:tt)*) => {
        ::std::result::Result::Err(::syn::Error::new(
            ::proc_macro2::Span::call_site(),
            ::std::concat!(
                "`",
                ::std::stringify!($v),
                "` takes arguments - write it as `",
                ::std::stringify!($v),
                "(..)`"
            ),
        ))
    };
    (@from_path named $name:ident $v:ident $($rest:tt)*) => {
        ::std::result::Result::Err(::syn::Error::new(
            ::proc_macro2::Span::call_site(),
            ::std::concat!(
                "`",
                ::std::stringify!($v),
                "` takes arguments - write it as `",
                ::std::stringify!($v),
                "(..)`"
            ),
        ))
    };

    // --- reading a variant written as a list --------------------------------
    (@from_list unit $name:ident $key:literal $body:ident $span:ident { } $v:ident) => {
        ::std::result::Result::Err(::syn::Error::new(
            $span,
            ::std::concat!("`", $key, "` takes no arguments"),
        ))
    };

    (@from_list positional $name:ident $key:literal $body:ident $span:ident
        { $($t:ty),+ } $v:ident ( $($_d:tt)* )
    ) => {{
        let arity = <[()]>::len(&[ $($crate::variants!(@unit_of $t)),+ ]);
        let exprs = $body.positional(arity, $key)?;
        let mut taken = exprs.into_iter();

        // BUBBLES. `positional` already checked the count, so None is unreachable - but an
        // unreachable panic in the AUTHOR'S compile is still a panic, and a diagnostic naming the
        // framework costs nothing. See NOTE(#vocab/no-panics-in-generated-code).
        ::std::result::Result::Ok($name::$v(
            $(
                <$t as $crate::vocab::leaves::FromExpr>::from_expr(&match taken.next() {
                    ::std::option::Option::Some(expr) => expr,
                    ::std::option::Option::None => {
                        return ::std::result::Result::Err(::syn::Error::new(
                            $span,
                            ::std::concat!(
                                "internal: `", $key, "` passed its arity check and then ran out \
                                 of arguments. This is a proc_macro_flow bug."
                            ),
                        ));
                    }
                })?
            ),+
        ))
    }};

    (@from_list named $name:ident $key:literal $body:ident $span:ident
        { $($f:ident : $t:ty),+ } $v:ident { $($_d:tt)* }
    ) => {{
        // The struct variant's key set, local to this reader - same reasoning as meta_list!.
        $crate::keys! {
            #[allow(non_camel_case_types)]
            enum Key { $( $f ),+ }
        }

        $( let mut $f: ::std::option::Option<$t> = ::std::option::Option::None; )+
        let mut errors = $crate::vocab::walk::Errors::new();

        errors.absorb($body.walk::<Key, _>(|written, element| {
            // Exhaustive; the `_ => {}` arm this replaced was the same silent fall-through
            // meta_list! carried.
            match written.key() {
                $(
                    Key::$f => {
                        $f = ::std::option::Option::Some(
                            <$t as $crate::vocab::leaves::FromMeta>::from_meta(element)?,
                        );
                    }
                )+
            }
            ::std::result::Result::Ok(())
        }));

        $(
            if $f.is_none() {
                errors.push(::syn::Error::new(
                    $span,
                    ::std::concat!(
                        "missing required key `",
                        ::std::stringify!($f),
                        "` of `",
                        $key,
                        "`"
                    ),
                ));
            }
        )+

        match errors.finish() {
            ::std::result::Result::Ok(()) => ::std::result::Result::Ok($name::$v {
                $( $f: match $f {
                    ::std::option::Option::Some(value) => value,
                    ::std::option::Option::None => {
                        return ::std::result::Result::Err(::syn::Error::new(
                            $span,
                            ::std::concat!(
                                "internal: `", ::std::stringify!($f), "` passed the required \
                                 check and then was not present. This is a proc_macro_flow bug."
                            ),
                        ));
                    }
                } ),+
            }),
            ::std::result::Result::Err(error) => ::std::result::Result::Err(error),
        }
    }};

    // counts one element per type, so arity comes from the declaration
    (@unit_of $t:ty) => { () };
}

#[cfg(test)]
mod tests {
    use crate::vocab::leaves::FromExpr;
    use syn::{parse_str, Attribute, Ident, ItemStruct, LitInt, Meta};

    variants! {
        /// Every variant shape at once.
        pub enum ColourSetting {
            Red = "Red",
            Other = "Other" (Ident),
            Rgb = "Rgb" { r: LitInt, g: LitInt, b: LitInt },
        }
    }

    fn meta_of(source: &str) -> Meta {
        let item: ItemStruct =
            parse_str(&format!("#[c({source})]\npub struct T;")).expect("the attribute parses");
        let attr: Attribute = item.attrs.into_iter().next().expect("one attribute");
        match attr.meta {
            Meta::List(list) => syn::parse2(list.tokens).expect("one meta inside"),
            _ => panic!("expected a list"),
        }
    }

    #[test]
    fn a_unit_variant_reads_from_a_bare_path() {
        assert!(matches!(
            ColourSetting::try_from(&meta_of("Red")).unwrap(),
            ColourSetting::Red
        ));
    }

    #[test]
    fn a_qualified_path_is_the_same_node() {
        // ID(variants/matched-not-resolved): suffix matching, so the canonical form works too.
        assert!(matches!(
            ColourSetting::try_from(&meta_of("ColourSetting::Red")).unwrap(),
            ColourSetting::Red
        ));
    }

    #[test]
    fn a_tuple_variant_reads_its_payload_positionally() {
        let got = ColourSetting::try_from(&meta_of("Other(Blue)")).unwrap();
        match got {
            ColourSetting::Other(ident) => assert_eq!(ident.to_string(), "Blue"),
            _ => panic!("expected Other"),
        }
    }

    #[test]
    fn a_struct_variant_reads_its_payload_by_key() {
        let got = ColourSetting::try_from(&meta_of("Rgb(r = 12, g = 34, b = 56)")).unwrap();
        match got {
            ColourSetting::Rgb { r, g, b } => {
                assert_eq!(r.base10_parse::<u8>().unwrap(), 12);
                assert_eq!(g.base10_parse::<u8>().unwrap(), 34);
                assert_eq!(b.base10_parse::<u8>().unwrap(), 56);
            }
            _ => panic!("expected Rgb"),
        }
    }

    #[test]
    fn variant_arity_comes_from_the_declaration() {
        let error = ColourSetting::try_from(&meta_of("Other(Blue, Teal)"))
            .err()
            .expect("takes one");
        assert_eq!(error.to_string(), "`Other` takes 1 argument, 2 supplied");
    }

    #[test]
    fn an_unknown_variant_carries_the_candidate_list() {
        let error = ColourSetting::try_from(&meta_of("Teal"))
            .err()
            .expect("no such variant");
        assert_eq!(error.to_string(), "expected one of: `Red`, `Other`, `Rgb`");
    }

    #[test]
    fn a_unit_variant_written_with_arguments_says_so() {
        let error = ColourSetting::try_from(&meta_of("Red(x)"))
            .err()
            .expect("takes none");
        assert!(error.to_string().contains("takes no arguments"), "{error}");
    }

    #[test]
    fn a_payload_variant_written_bare_says_so() {
        let error = ColourSetting::try_from(&meta_of("Other"))
            .err()
            .expect("needs arguments");
        assert!(error.to_string().contains("takes arguments"), "{error}");
    }

    #[test]
    fn a_struct_variant_missing_a_key_is_reported() {
        let error = ColourSetting::try_from(&meta_of("Rgb(r = 12)"))
            .err()
            .expect("g and b missing");
        assert_eq!(error.into_iter().count(), 2, "both missing keys reported");
    }

    #[test]
    fn a_unit_variant_is_also_readable_from_an_expr() {
        // `colour(Red)` parses its body as Exprs, so this is the path that actually fires there.
        let expr: syn::Expr = parse_str("Red").unwrap();
        assert!(matches!(
            ColourSetting::from_expr(&expr).unwrap(),
            ColourSetting::Red
        ));
    }

    #[test]
    fn the_variant_list_comes_from_the_declaration() {
        assert_eq!(ColourSetting::NAMES, &["Red", "Other", "Rgb"]);
    }
}
