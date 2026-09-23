// @review [ ]
//! The shared spine walk, and the error accumulator that keeps siblings alive.
//!
//! `meta_list!` and `variants!` both emit a key dispatch and call in here; neither writes its own
//! walk. One walker means `UnknownKey`, `Duplicate` and the candidate list are decided once, so a
//! diagnostics fix lands everywhere instead of in N expansions.
//!
//! NOTE(#walk/accumulates): V[S(Errors).F(combine) && !F(walk_keys).uses(?)], "The walk does NOT use
//! `?` on an element. VERIFIED what that would cost: a syn Parse impl using `?` over three bad
//! elements reports ONE of them and silently drops the other two, which is exactly what ID(no-result)
//! exists to prevent. syn::Error::combine keeps each error's own span and to_compile_error emits one
//! compile_error! per error, so accumulating here costs nothing and loses nothing. The only thing
//! that aborts is a body that will not parse as a meta list AT ALL - there are no siblings to lose
//! when there are no siblings"
//!
//! Fix[x](#walk/suffix):R[F(walk).requires(Ident)], "SETTLED, and it settled a CONTRADICTION rather
//! than a preference. This note used to say a key is matched on its LAST PATH SEGMENT, so `colour`
//! and `some::colour` were the same key, justified as ID(vocab/match-or-splice)'s cost in its
//! narrowest form.
//!
//! ID(resolve) says the opposite, in as many words: `Keys are idents and values are paths - exactly
//! the asymmetry Rust already has in Foo { bar: Baz::Qux }. Fields are not items, so there is no
//! configuration::colour to resolve and THE QUALIFIED KEY FORM IS DROPPED`. Both were written down;
//! only one can be true of the walker.
//!
//! ID(resolve) wins, and F(walk) enforces it: a key must be a bare Ident (`path().get_ident()`), so
//! a qualified key is now an unknown key rather than a silently accepted one. That is also the
//! ID(type-backed) answer - `get_ident` is a type-level question with a yes/no answer, where
//! last-segment matching was a string operation that quietly discarded what the author wrote.
//!
//! VALUES still match by suffix. That is ID(resolve)'s other half and is untouched here"
//! `colour` and `some::colour` are the same key. That is ID(vocab/match-or-splice)'s documented
//! cost in its narrowest form: we match a name because the caller needs a VALUE, and the price is
//! that a renamed import is not seen. For KEYS the price is near zero - a key is a name we invented
//! and nobody imports it"

use syn::{Error, Meta, Result};

use crate::meta::ListBody;

/// A closed set of names, and the one place a spelling is compared.
///
/// NOTE(#keys/table-is-strings-the-rest-is-not): V[C(spellings).T(str) && M(resolve).once],
/// "VERIFIED that the table CANNOT be typed tokens, before designing around the limitation:
/// proc_macro2::Ident::new(&str, Span) is not const and Span::call_site() is not const, so
/// `const SPELLINGS: &[Ident]` cannot exist at all. A spelling is a &'static str at rest and there
/// is no way around that.
///
/// What ID(type-backed) asks for is still reachable, and it is what this trait is shaped for: the
/// string is compared in EXACTLY ONE place - F(resolve) - and everything downstream carries the
/// typed variant plus the author's own token. `impl<T: AsRef<str>> PartialEq<T> for Ident` makes
/// that comparison `ident == spelling` with no String and no allocation, and it ignores spans,
/// which is right: two idents spelled the same ARE the same key however they were written"
pub trait Keys: Sized + Copy + PartialEq + 'static {
    /// The token this kind of name is written as.
    ///
    /// `syn::Ident` for the keys of a list, `syn::Path` for a variant name. Keys are idents and
    /// values are paths - the asymmetry Rust itself has in `Foo { bar: Baz::Qux }` - so one
    /// contract covers both by naming the difference rather than by having two traits.
    type Written: ?Sized;

    /// Every name in the set, in declaration order.
    const ALL: &'static [Self];

    /// The canonical spelling. Aliases are accepted on the way in and never produced.
    fn canonical(self) -> &'static str;

    /// Every accepted spelling for this name, canonical first.
    fn spellings(self) -> &'static [&'static str];

    /// Resolve a written token to the name it spells, or `None` if it spells none of them.
    fn resolve(written: &Self::Written) -> Option<Self>;
}

/// Declare a LOCAL key set and its [`Keys`] impl.
///
/// NOTE(#keys/two-arms): V[M(keys).arm(literal) && M(keys).arm(ident)], "Two arms, because the two
/// callers have different information and macro_rules cannot bridge them. M(meta_list) knows an
/// explicit spelling per field (`colour: T = \"color\"`) so it passes literals. M(variants)' struct
/// variants have only the FIELD IDENT, and turning an ident into a literal is precisely what a
/// declarative macro cannot do - the same wall ID(vocabulary/derive-spelling) hit. `stringify!` in
/// EXPRESSION position works, which is why the ident arm exists at all; it is the `:literal`
/// MATCHER that cannot accept it.
///
/// Separate from M(vocabulary) on purpose: that one is the public, aliased vocabulary with
/// TryFrom conversions, this one is a private key set local to a single reader"
#[macro_export]
#[doc(hidden)]
macro_rules! keys {
    // Explicit spellings.
    ( $(#[$meta:meta])* enum $name:ident { $( $variant:ident = $spelling:literal ),+ $(,)? } ) => {
        $crate::keys!(@emit $(#[$meta])* $name { $( $variant , $spelling ),+ });
    };

    // Spellings derived from the field idents.
    ( $(#[$meta:meta])* enum $name:ident { $( $variant:ident ),+ $(,)? } ) => {
        $crate::keys!(@emit $(#[$meta])* $name {
            $( $variant , ::std::stringify!($variant) ),+
        });
    };

    (@emit $(#[$meta:meta])* $name:ident { $( $variant:ident , $spelling:expr ),+ }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum $name { $( $variant ),+ }

        impl $crate::vocab::walk::Keys for $name {
            type Written = ::syn::Ident;

            const ALL: &'static [Self] = &[ $( $name::$variant ),+ ];

            fn canonical(self) -> &'static str {
                match self { $( $name::$variant => $spelling ),+ }
            }

            fn spellings(self) -> &'static [&'static str] {
                match self { $( $name::$variant => &[$spelling] ),+ }
            }

            fn resolve(written: &::syn::Ident) -> ::std::option::Option<Self> {
                $( if written == $spelling { return ::std::option::Option::Some($name::$variant); } )+
                ::std::option::Option::None
            }
        }
    };
}

/// A resolved name, together with the token that spelled it.
///
/// The point of carrying both: `key` is typed so a caller's match is exhaustive, and `token` is
/// the AUTHOR'S, so any error raised about this name underlines what they actually wrote instead
/// of a span reconstructed afterwards.
pub struct Written<'ast, K: Keys> {
    key: K,
    token: &'ast K::Written,
}

impl<'ast, K: Keys> Written<'ast, K> {
    pub fn new(key: K, token: &'ast K::Written) -> Self {
        Self { key, token }
    }

    /// Which name this is. Match on it; there is no catch-all to fall through.
    pub fn key(self) -> K {
        self.key
    }

    /// What the author wrote, with the author's span.
    pub fn token(self) -> &'ast K::Written {
        self.token
    }
}

impl<K: Keys> Clone for Written<'_, K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: Keys> Copy for Written<'_, K> {}

/// An error accumulator.
///
/// `push` never discards what came before, so a walk can keep going after a bad element and still
/// report every complaint at the end.
#[derive(Default)]
pub struct Errors(Option<Error>);

impl Errors {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn push(&mut self, error: Error) {
        match &mut self.0 {
            Some(existing) => existing.combine(error),
            slot @ None => *slot = Some(error),
        }
    }

    /// Absorb a fallible step, keeping any error and reporting whether it succeeded.
    pub fn absorb<T>(&mut self, result: Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                self.push(error);
                None
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    pub fn finish(self) -> Result<()> {
        match self.0 {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Finish, yielding a value only if nothing went wrong.
    pub fn finish_with<T>(self, value: T) -> Result<T> {
        self.finish().map(|()| value)
    }
}

/// Naming, on the syn types themselves.
///
/// An extension trait rather than free functions, per NOTE(#pipeline/no-free-functions) - and it
/// has to be a trait because `syn::Path` is foreign, which is the same orphan-rule shape the whole
/// vocab suite is built around (ID(vocab/orphan-shapes-the-api)).
pub trait Named {
    /// The last segment, which is how both keys and variant names are matched.
    fn last_segment(&self) -> Option<String>;
}

impl Named for syn::Path {
    fn last_segment(&self) -> Option<String> {
        self.segments.last().map(|s| s.ident.to_string())
    }
}

impl Named for Meta {
    /// The key a `Meta` is written under.
    fn last_segment(&self) -> Option<String> {
        self.path().last_segment()
    }
}

/// The did-you-mean list, rendered from the table.
///
/// Strings are the OUTPUT here, which is the one place ID(keys/table-is-strings-the-rest-is-not)
/// permits them: a message is text. Nothing resolves against this.
fn expected_one_of_keys<K: Keys>() -> String {
    let quoted: Vec<String> = K::ALL
        .iter()
        .flat_map(|key| key.spellings().iter())
        .map(|spelling| format!("`{spelling}`"))
        .collect();
    format!("expected one of: {}", quoted.join(", "))
}

/// Walk a list body, dispatching each element by its key.
///
/// Owns unknown-key and duplicate-key reporting; the caller's closure only has to read the element
/// it was handed. Missing required keys are NOT reported here - which fields are required is the
/// caller's business, because it is written in their types.
impl<'ast> ListBody<'ast> {
    /// Walk this body, dispatching each element by its resolved key.
    ///
    /// The closure receives a Ty(Written) - the typed name AND the token that spelled it - so a
    /// caller's match is exhaustive and every error it raises already has the author's span. That
    /// is the whole difference from the string-shaped `walk_keys` this REPLACED, which handed back
    /// a `&str` and left both to the caller. That function is deleted rather than deprecated -
    /// keeping both is how the two vocabularies grow back apart.
    ///
    /// Owns unknown-key and duplicate-key reporting; the closure only reads the element it was
    /// handed. Missing required keys are NOT reported here - which are required is written in the
    /// caller's TYPES, per ID(from/arity-from-type), and this walker cannot see them.
    /// NOTE(#walk/metas-are-owned): V[F(metas).R(owned)], "The closure sees `&Meta` and
    /// `Written<'_, K>` at a LOCAL lifetime, not at 'ast, and that is forced rather than chosen.
    /// F(metas) PARSES the carried tokens, so every Meta it yields is a new value - there is no
    /// `&'ast Meta` in the AST to hand out, because the body was never parsed until now
    /// (ID(no-parse)). Span fidelity is unaffected: syn carries the original spans through
    /// parsing, so an error against one of these idents still underlines what the author wrote.
    /// Only the LIFETIME is local; the span is the real one"
    pub fn walk<K, F>(self, mut accept: F) -> Result<()>
    where
        K: Keys<Written = syn::Ident>,
        F: FnMut(Written<'_, K>, &Meta) -> Result<()>,
    {
        // The one thing that aborts: a body that is not a meta list has no elements to salvage.
        let metas = self.metas()?;

        let mut errors = Errors::new();
        // `K: Copy + PartialEq`, so duplicate detection is a comparison over the TYPE. It used to
        // be `Vec<String>::contains`, which compared spellings and so could not tell an alias from
        // the canonical name it aliases.
        let mut seen: Vec<K> = Vec::new();

        for meta in &metas {
            let Some(ident) = meta.path().get_ident() else {
                errors.push(Error::new_spanned(meta.path(), expected_one_of_keys::<K>()));
                continue;
            };

            let Some(key) = K::resolve(ident) else {
                errors.push(Error::new_spanned(ident, expected_one_of_keys::<K>()));
                continue;
            };

            if seen.contains(&key) {
                errors.push(Error::new_spanned(
                    ident,
                    format!("`{}` is written more than once", key.canonical()),
                ));
                continue;
            }

            seen.push(key);
            errors.absorb(accept(Written::new(key, ident), meta));
        }

        errors.finish()
    }

    /// Read this body as positional values - what a tuple variant's payload is.
    ///
    /// `bounds(0, 64)` cannot go through [`ListBody::walk_keys`]: a bare literal is not valid
    /// `Meta` at all, so there is no key to dispatch on. Arity is checked here because a tuple's
    /// arity is fixed by its declaration, not by what was written.
    pub fn positional(self, arity: usize, what: &str) -> Result<Vec<syn::Expr>> {
        let body = self;
        let exprs = body.exprs()?;

        if exprs.len() != arity {
            return Err(Error::new_spanned(
                body,
                format!(
                    "`{what}` takes {arity} argument{}, {} supplied",
                    if arity == 1 { "" } else { "s" },
                    exprs.len()
                ),
            ));
        }

        Ok(exprs.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{ListBody, Opening};
    use syn::{parse_str, Attribute, ItemStruct};

    fn body_of(source: &str) -> (Attribute, ()) {
        let item: ItemStruct =
            parse_str(&format!("{source}\npub struct T;")).expect("the attribute parses");
        (item.attrs.into_iter().next().expect("one attribute"), ())
    }

    fn with_body<R>(source: &str, f: impl FnOnce(ListBody<'_>) -> R) -> R {
        let (attr, ()) = body_of(source);
        match Opening::from(&attr.meta) {
            Opening::List(body) => f(body),
            _ => panic!("expected a list"),
        }
    }


    // ---- the typed walk -----------------------------------------------------------------

    crate::vocabulary! {
        /// A key set with an alias, so the tests can tell canonical from accepted.
        enum Key {
            Colour = "colour" | "color",
            Name = "name",
        }
    }

    #[test]
    fn a_key_resolves_to_its_variant_and_keeps_the_token() {
        let mut seen = Vec::new();
        with_body("#[t(colour(a), name = 1)]", |body| {
            body.walk::<Key, _>(|written, _| {
                // the TOKEN is the author's, and the KEY is typed
                seen.push((written.key(), written.token().to_string()));
                Ok(())
            })
        })
        .expect("both keys are known");

        assert_eq!(seen, vec![(Key::Colour, "colour".into()), (Key::Name, "name".into())]);
    }

    #[test]
    fn an_alias_resolves_to_the_canonical_variant() {
        // `color` is accepted on the way in and never produced - the VARIANT is what the caller
        // matches on, so an alias is indistinguishable downstream. That is the point.
        let mut keys = Vec::new();
        with_body("#[t(color(a))]", |body| {
            body.walk::<Key, _>(|written, _| {
                keys.push(written.key());
                // the token still says what they wrote
                assert_eq!(written.token().to_string(), "color");
                Ok(())
            })
        })
        .expect("the alias is accepted");

        assert_eq!(keys, vec![Key::Colour]);
        assert_eq!(Key::Colour.canonical(), "colour");
    }

    #[test]
    fn a_duplicate_is_caught_across_an_alias() {
        // THE gain from comparing K instead of spellings: `colour` and `color` are the SAME key,
        // so writing both is a duplicate. The string-shaped walker compared spellings and could
        // not see it.
        let error = with_body("#[t(colour(a), color(b))]", |body| {
            body.walk::<Key, _>(|_, _| Ok(()))
        })
        .expect_err("the same key written twice");

        assert!(error.to_string().contains("more than once"), "{error}");
    }

    #[test]
    fn an_unknown_key_lists_every_accepted_spelling() {
        let error = with_body("#[t(colur(a))]", |body| body.walk::<Key, _>(|_, _| Ok(())))
            .expect_err("not a key");
        let message = error.to_string();

        assert!(message.contains("`colour`"), "{message}");
        assert!(message.contains("`color`"), "{message}");
        assert!(message.contains("`name`"), "{message}");
    }

    #[test]
    fn every_bad_key_is_reported_not_just_the_first() {
        // ID(walk/accumulates), through the typed path.
        let error = with_body("#[t(a(1), b(2), c(3))]", |body| {
            body.walk::<Key, _>(|_, _| Ok(()))
        })
        .expect_err("three unknown keys");

        assert_eq!(error.into_iter().count(), 3);
    }

    #[test]
    fn a_qualified_key_is_not_a_key() {
        // Fix[x](#walk/suffix). `some::colour` used to resolve as `colour` by last-segment match;
        // ID(resolve) says the qualified key form is DROPPED, and this walker enforces it.
        let error = with_body("#[t(some::colour(a))]", |body| {
            body.walk::<Key, _>(|_, _| Ok(()))
        })
        .expect_err("a qualified key is not one of ours");

        assert!(error.to_string().contains("expected one of"), "{error}");
    }

    #[test]
    fn the_walk_continues_past_an_unknown_key() {
        // ID(walk/accumulates): one bad element must not cost the good ones.
        let mut keys = Vec::new();
        let error = with_body("#[t(nope(1), name = 2)]", |body| {
            body.walk::<Key, _>(|written, _| {
                keys.push(written.key());
                Ok(())
            })
        })
        .expect_err("one key is unknown");

        assert_eq!(keys, vec![Key::Name], "the good key was not dispatched");
        assert_eq!(error.into_iter().count(), 1);
    }

    #[test]
    fn an_error_from_the_callback_is_kept_too() {
        let error = with_body("#[t(colour(a), name = 2)]", |body| {
            body.walk::<Key, _>(|written, _| match written.key() {
                Key::Colour => Err(syn::Error::new_spanned(written.token(), "colour is unhappy")),
                Key::Name => Ok(()),
            })
        })
        .expect_err("the callback failed");

        assert!(error.to_string().contains("colour is unhappy"), "{error}");
    }

    #[test]
    fn a_duplicate_reports_once_and_the_first_wins() {
        let mut count = 0usize;
        let error = with_body("#[t(name = 1, name = 2)]", |body| {
            body.walk::<Key, _>(|_, _| {
                count += 1;
                Ok(())
            })
        })
        .expect_err("written twice");

        assert_eq!(count, 1, "the second was dispatched as well as reported");
        assert_eq!(error.into_iter().count(), 1);
    }

    #[test]
    fn a_body_that_is_not_a_meta_list_aborts() {
        // The ONE thing that aborts rather than accumulates - there are no siblings to lose when
        // there are no siblings. See NOTE(#walk/accumulates).
        let result = with_body("#[t(0, 64)]", |body| body.walk::<Key, _>(|_, _| Ok(())));
        assert!(result.is_err());
    }

    #[test]
    fn positional_checks_arity_against_the_declaration() {
        with_body("#[c(0, 64)]", |body| {
            assert!(matches!(body.positional(2, "Bounds"), Ok(exprs) if exprs.len() == 2));
        });
        // `.err().expect(..)` rather than `expect_err` - syn types carry no Debug without the
        // `extra-traits` feature, and turning that on for every user to please a test is the wrong
        // way round.
        let error = with_body("#[c(0)]", |body| body.positional(2, "Bounds"))
            .err()
            .expect("one argument, two declared");
        assert!(error.to_string().contains("2 arguments"), "{error}");
    }
}
