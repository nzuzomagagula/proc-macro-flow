// @review [ ]
//! `meta_list!` - a struct whose body is more named nodes.
//!
//! Closes the spine half of ID(attribute/name-value) and is what ID(attribute/list) meant by "a
//! struct (named fields)".
//!
//! NOTE(#meta-list/requiredness-is-written): V[M(meta_list).matches(Option<_>)], "Requiredness is
//! read off the field's WRITTEN type: `backoff: Option<LitStr>` is optional, `times: LitInt` is
//! not. There is no #[required] and no second vocabulary for arity, which is ID(syntax/type)'s rule
//! kept intact. The mechanism is syntactic, not type inspection - macro_rules matches the tokens
//! `Option < .. >` before it matches a bare type, so a field spelled
//! `std::option::Option<LitStr>` would be treated as REQUIRED. Nothing can detect that; the derive
//! will not have the limitation because a proc macro sees the parsed type"
//!
//! NOTE(#meta-list/uniform-read): V[F(from_meta).per_field], "Every field is read the same way -
//! `<FieldTy as FromMeta>::from_meta(meta)` - whatever shape it is. A flag reads a Meta::Path, a
//! leaf the right-hand side of `=`, a nested list its own body, and this macro does not know or
//! care which. That is why ID(leaves/uniform-field-read) exists and why there is no shape dispatch
//! here"

/// Declare a struct read from a list body.
///
/// ```ignore
/// meta_list! {
///     pub struct Retry {
///         times: LitInt = "times",
///         backoff: Option<LitStr> = "backoff",
///     }
/// }
/// ```
///
/// Generates the struct plus `FromMeta`, `TryFrom<&syn::Meta>` and `TryFrom<ListBody>`. The walk,
/// the unknown-key and duplicate-key reporting all come from [`crate::vocab::walk::walk_keys`];
/// what is generated per type is the key list and the dispatch, both from this one declaration.
#[macro_export]
macro_rules! meta_list {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $($fields:tt)*
        }
    ) => {
        $crate::meta_list!(@munch
            head { $(#[$meta])* $vis $name }
            done { }
            rest { $($fields)* }
        );
    };

    // An Option field is optional. This rule must precede the bare-type one: `$ty:ty` would
    // otherwise swallow `Option<LitStr>` whole. See ID(meta-list/requiredness-is-written).
    (@munch
        head { $($head:tt)* }
        done { $($done:tt)* }
        rest { $field:ident : Option<$ty:ty> = $key:literal , $($rest:tt)* }
    ) => {
        $crate::meta_list!(@munch
            head { $($head)* }
            done { $($done)* [$field, $ty, $key, optional] }
            rest { $($rest)* }
        );
    };

    (@munch
        head { $($head:tt)* }
        done { $($done:tt)* }
        rest { $field:ident : $ty:ty = $key:literal , $($rest:tt)* }
    ) => {
        $crate::meta_list!(@munch
            head { $($head)* }
            done { $($done)* [$field, $ty, $key, required] }
            rest { $($rest)* }
        );
    };

    (@munch
        head { $(#[$meta:meta])* $vis:vis $name:ident }
        done { $([$field:ident, $ty:ty, $key:literal, $req:ident])* }
        rest { }
    ) => {
        $(#[$meta])*
        #[derive(Clone)]
        $vis struct $name {
            $( $vis $field: $crate::meta_list!(@ty $req $ty), )*
        }

        #[allow(dead_code)]
        impl $name {
            /// Every key this node accepts, in declaration order.
            pub const KEYS: &'static [&'static str] = &[ $($key),* ];
        }

        impl $crate::vocab::leaves::FromMeta for $name {
            fn from_meta(meta: &::syn::Meta) -> ::syn::Result<Self> {
                <$name as ::std::convert::TryFrom<&::syn::Meta>>::try_from(meta)
            }
        }

        impl ::std::convert::TryFrom<&::syn::Meta> for $name {
            type Error = ::syn::Error;

            fn try_from(meta: &::syn::Meta) -> ::std::result::Result<Self, Self::Error> {
                match meta {
                    ::syn::Meta::List(list) => {
                        <$name as ::std::convert::TryFrom<$crate::meta::ListBody<'_>>>::try_from(
                            $crate::meta::ListBody(&list.tokens),
                        )
                    }
                    other => ::std::result::Result::Err(::syn::Error::new_spanned(
                        other,
                        ::std::concat!(
                            "expected `",
                            ::std::stringify!($name),
                            "` to be written as a list"
                        ),
                    )),
                }
            }
        }

        impl<'ast> ::std::convert::TryFrom<$crate::meta::ListBody<'ast>> for $name {
            type Error = ::syn::Error;

            fn try_from(
                body: $crate::meta::ListBody<'ast>,
            ) -> ::std::result::Result<Self, Self::Error> {
                $( let mut $field: ::std::option::Option<$ty> = ::std::option::Option::None; )*

                let mut errors = $crate::vocab::walk::Errors::new();

                // The walk owns unknown-key and duplicate reporting; this closure only reads the
                // element it was handed. Every field reads the same way whatever shape it is -
                // see ID(meta-list/uniform-read).
                errors.absorb(body.walk_keys($name::KEYS,
                    |key, element| {
                        match key {
                            $(
                                $key => {
                                    $field = ::std::option::Option::Some(
                                        <$ty as $crate::vocab::leaves::FromMeta>::from_meta(
                                            element,
                                        )?,
                                    );
                                }
                            )*
                            // walk_keys only dispatches keys it found in KEYS, so this is
                            // unreachable rather than a case to handle.
                            _ => {}
                        }
                        ::std::result::Result::Ok(())
                    },
                ));

                // Requiredness is enforced HERE and not in the walk: which fields are required is
                // written in their types, which the walk cannot see.
                $(
                    $crate::meta_list!(@missing $req errors, $field, $key, body);
                )*

                if errors.is_empty() {
                    ::std::result::Result::Ok($name {
                        $( $field: $crate::meta_list!(@take $req $field, $key), )*
                    })
                } else {
                    ::std::result::Result::Err(
                        errors.finish().err().expect("not empty"),
                    )
                }
            }
        }
    };

    // --- small helpers, so the arms above stay readable ---------------------
    (@ty required $ty:ty) => { $ty };
    (@ty optional $ty:ty) => { ::std::option::Option<$ty> };

    (@missing required $errors:ident, $field:ident, $key:literal, $body:ident) => {
        if $field.is_none() {
            $errors.push(::syn::Error::new_spanned(
                $body,
                ::std::concat!("missing required key `", $key, "`"),
            ));
        }
    };
    (@missing optional $errors:ident, $field:ident, $key:literal, $body:ident) => {};

    (@take required $field:ident, $key:literal) => {
        $field.expect(::std::concat!("`", $key, "` was checked present"))
    };
    (@take optional $field:ident, $key:literal) => { $field };
}

#[cfg(test)]
mod tests {
    use crate::meta::Opening;
    use syn::{parse_str, Attribute, ItemStruct, LitInt, LitStr, Meta};

    meta_list! {
        /// `retry(times = 3, backoff = "200ms")`
        pub struct Retry {
            times: LitInt = "times",
            backoff: Option<LitStr> = "backoff",
        }
    }

    fn meta_of(source: &str) -> Meta {
        let item: ItemStruct =
            parse_str(&format!("{source}\npub struct T;")).expect("the attribute parses");
        let attr: Attribute = item.attrs.into_iter().next().expect("one attribute");
        attr.meta
    }

    #[test]
    fn a_list_reads_into_its_struct() {
        let retry = Retry::try_from(&meta_of(r#"#[retry(times = 3, backoff = "200ms")]"#)).unwrap();

        assert_eq!(retry.times.base10_parse::<u32>().unwrap(), 3);
        assert_eq!(retry.backoff.unwrap().value(), "200ms");
    }

    #[test]
    fn an_optional_key_may_be_absent() {
        let retry = Retry::try_from(&meta_of("#[retry(times = 1)]")).unwrap();
        assert!(retry.backoff.is_none());
    }

    #[test]
    fn a_missing_required_key_is_reported() {
        let error = Retry::try_from(&meta_of(r#"#[retry(backoff = "200ms")]"#))
            .err()
            .expect("times is required");
        assert!(
            error.to_string().contains("missing required key `times`"),
            "{error}"
        );
    }

    #[test]
    fn an_unknown_key_carries_the_declared_candidates() {
        let error = Retry::try_from(&meta_of("#[retry(times = 1, tmies = 2)]"))
            .err()
            .expect("tmies is unknown");
        assert_eq!(error.to_string(), "expected one of: `times`, `backoff`");
    }

    #[test]
    fn a_duplicate_key_is_reported() {
        let error = Retry::try_from(&meta_of("#[retry(times = 1, times = 2)]"))
            .err()
            .expect("written twice");
        assert!(error.to_string().contains("more than once"), "{error}");
    }

    #[test]
    fn several_complaints_all_survive() {
        // A missing required key AND an unknown one: both reported, neither masks the other.
        let error = Retry::try_from(&meta_of("#[retry(nope = 1)]"))
            .err()
            .expect("two problems");
        assert_eq!(error.into_iter().count(), 2);
    }

    #[test]
    fn a_wrong_leaf_is_the_leafs_own_complaint() {
        let error = Retry::try_from(&meta_of(r#"#[retry(times = "three")]"#))
            .err()
            .expect("not an int");
        assert!(error.to_string().contains("Int literal"), "{error}");
    }

    #[test]
    fn written_as_the_wrong_shape_says_so() {
        let error = Retry::try_from(&meta_of("#[retry = 1]"))
            .err()
            .expect("not a list");
        assert!(error.to_string().contains("written as a list"), "{error}");
    }

    #[test]
    fn the_key_list_comes_from_the_declaration() {
        assert_eq!(Retry::KEYS, &["times", "backoff"]);
    }

    #[test]
    fn a_body_can_be_read_directly() {
        // The ListBody entry point, which is what a nested field will use.
        let meta = meta_of("#[retry(times = 7)]");
        let Opening::List(body) = Opening::from(&meta) else {
            panic!("a list")
        };
        let retry = Retry::try_from(body).unwrap();
        assert_eq!(retry.times.base10_parse::<u32>().unwrap(), 7);
    }

    #[test]
    fn a_nested_list_composes() {
        // Retry implements FromMeta, so it can be a FIELD of another meta_list! - the uniform read
        // means the outer struct writes the same line for it as for a leaf.
        meta_list! {
            pub struct Outer {
                retry: Retry = "retry",
                label: Option<LitStr> = "label",
            }
        }

        let outer =
            Outer::try_from(&meta_of(r#"#[outer(retry(times = 2), label = "x")]"#)).unwrap();
        assert_eq!(outer.retry.times.base10_parse::<u32>().unwrap(), 2);
        assert_eq!(outer.label.unwrap().value(), "x");
    }
}
