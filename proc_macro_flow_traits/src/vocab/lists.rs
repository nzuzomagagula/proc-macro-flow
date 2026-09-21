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

        impl $crate::node::Described for $name {
            /// The reflection table, built from the SAME declaration the key enum is.
            ///
            /// Replaces the `const KEYS: &[&str]` this macro used to emit. That const was a
            /// second list of the same names with nothing holding the two in step; a Ty(Node) is
            /// the shape ID(diagnostics) and the Node half of ID(reason) actually need, and it
            /// carries arity, which a bare key list could not. See NOTE(#keys/one-table).
            const NODE: $crate::node::Node = $crate::node::Node {
                name: ::std::stringify!($name),
                children: &[
                    $(
                        $crate::node::Child {
                            key: $key,
                            // TODO[ ](#meta-list/aliases): M(meta_list) has no alias syntax yet -
                            // ID(alias-attr) is the author-facing surface and lands with the
                            // derive. The field is here so Ty(Node) does not change shape when it
                            // does.
                            aliases: &[],
                            arity: $crate::meta_list!(@arity $req),
                            // Shapes are the FIELD TYPE's business and this macro cannot see
                            // them - ID(shape-attr) supplies them from the derive.
                            shapes: &[],
                        },
                    )*
                ],
            };
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

                // The key set, as a LOCAL type. Declaring it in the function body sidesteps the
                // one thing macro_rules cannot do - build an ident like `RetryKey` - and it is
                // better than the workaround would have been: the enum is private to the reader
                // that uses it, so there is no second public name to keep in step.
                $crate::keys! {
                    #[allow(non_camel_case_types)]
                    enum Key { $( $field = $key ),* }
                }

                // The walk owns unknown-key and duplicate reporting; this closure only reads the
                // element it was handed. Every field reads the same way whatever shape it is -
                // see ID(meta-list/uniform-read).
                errors.absorb(body.walk::<Key, _>(
                    |written, element| {
                        // EXHAUSTIVE. This match had a `_ => {}` arm when the walk dispatched on
                        // &str, defended as unreachable because walk_keys only ever handed back a
                        // key it had found in KEYS. True, but it meant the two lists agreeing was
                        // a property nothing checked. Over a typed key there is no arm to write.
                        match written.key() {
                            $(
                                Key::$field => {
                                    $field = ::std::option::Option::Some(
                                        <$ty as $crate::vocab::leaves::FromMeta>::from_meta(
                                            element,
                                        )?,
                                    );
                                }
                            )*
                        }
                        ::std::result::Result::Ok(())
                    },
                ));

                // Requiredness is enforced HERE and not in the walk: which fields are required is
                // written in their types, which the walk cannot see.
                $(
                    $crate::meta_list!(@missing $req errors, $field, $key, body);
                )*

                // `errors.finish()` is CONSUMED to decide, rather than tested with is_empty and
                // then unwrapped. The old shape called `.err().expect("not empty")` in the else
                // arm - a panic standing on an invariant two lines apart, in code that runs in the
                // AUTHOR'S compile. See NOTE(#vocab/no-panics-in-generated-code).
                match errors.finish() {
                    ::std::result::Result::Ok(()) => ::std::result::Result::Ok($name {
                        $( $field: $crate::meta_list!(@take $req $field, $key, body)?, )*
                    }),
                    ::std::result::Result::Err(error) => ::std::result::Result::Err(error),
                }
            }
        }
    };

    // --- small helpers, so the arms above stay readable ---------------------
    (@arity required) => { $crate::node::Arity::Required };
    (@arity optional) => { $crate::node::Arity::Optional };
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

    // BUBBLES rather than panics. Reaching the None arm would mean @missing and @take disagree,
    // which is a framework bug and not bad user input - but a diagnostic naming it beats a panic
    // in the middle of expansion, which is all the author would otherwise see.
    (@take required $field:ident, $key:literal, $body:ident) => {
        match $field {
            ::std::option::Option::Some(value) => ::std::result::Result::Ok(value),
            ::std::option::Option::None => ::std::result::Result::Err(::syn::Error::new_spanned(
                $body,
                ::std::concat!(
                    "internal: `", $key, "` passed the required check and then was not present. \
                     This is a proc_macro_flow bug, not a mistake in this attribute."
                ),
            )),
        }
    };
    (@take optional $field:ident, $key:literal, $body:ident) => {
        ::std::result::Result::<_, ::syn::Error>::Ok($field)
    };
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
    fn the_node_table_comes_from_the_declaration() {
        use crate::node::Described;
        let node = <Retry as Described>::NODE;

        assert_eq!(node.name, "Retry");
        let keys: Vec<&str> = node.children.iter().map(|child| child.key).collect();
        assert_eq!(keys, ["times", "backoff"]);
    }

    #[test]
    fn the_node_table_carries_arity_a_key_list_could_not() {
        // The gain over the `const KEYS: &[&str]` this replaced: requiredness is IN the table, so
        // the framework can answer "what is missing" without the caller restating it.
        use crate::node::{Arity, Described};
        let node = <Retry as Described>::NODE;

        assert_eq!(node.child("times").unwrap().arity, Arity::Required);
        assert_eq!(node.child("backoff").unwrap().arity, Arity::Optional);

        let missing: Vec<&str> = node.missing(&[]).map(|child| child.key).collect();
        assert_eq!(missing, ["times"]);
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
