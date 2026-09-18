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
//! NOTE(#walk/suffix): V[F(key_of).segments.last()], "A key is matched on the LAST path segment, so
//! `colour` and `some::colour` are the same key. That is ID(vocab/match-or-splice)'s documented
//! cost in its narrowest form: we match a name because the caller needs a VALUE, and the price is
//! that a renamed import is not seen. For KEYS the price is near zero - a key is a name we invented
//! and nobody imports it"

use syn::{Error, Meta, Result};

use crate::meta::ListBody;

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

/// The last segment of a path, which is how both keys and variant names are matched.
pub fn last_segment(path: &syn::Path) -> Option<String> {
    path.segments.last().map(|s| s.ident.to_string())
}

/// The key a `Meta` is written under.
pub fn key_of(meta: &Meta) -> Option<String> {
    last_segment(meta.path())
}

fn expected_one_of(candidates: &[&'static str]) -> String {
    let quoted: Vec<_> = candidates.iter().map(|c| format!("`{c}`")).collect();
    format!("expected one of: {}", quoted.join(", "))
}

/// Walk a list body, dispatching each element by its key.
///
/// Owns unknown-key and duplicate-key reporting; the caller's closure only has to read the element
/// it was handed. Missing required keys are NOT reported here - which fields are required is the
/// caller's business, because it is written in their types.
pub fn walk_keys<F>(body: ListBody<'_>, candidates: &[&'static str], mut accept: F) -> Result<()>
where
    F: FnMut(&str, &Meta) -> Result<()>,
{
    // The one thing that aborts: a body that is not a meta list has no elements to salvage.
    let metas = body.metas()?;

    let mut errors = Errors::new();
    let mut seen: Vec<String> = Vec::new();

    for meta in &metas {
        let Some(key) = key_of(meta) else {
            errors.push(Error::new_spanned(meta, expected_one_of(candidates)));
            continue;
        };

        if !candidates.contains(&key.as_str()) {
            errors.push(Error::new_spanned(
                meta.path(),
                expected_one_of(candidates),
            ));
            continue;
        }

        if seen.contains(&key) {
            errors.push(Error::new_spanned(
                meta.path(),
                format!("`{key}` is written more than once"),
            ));
            continue;
        }

        seen.push(key.clone());
        errors.absorb(accept(&key, meta));
    }

    errors.finish()
}

/// Read a list body as positional values - what a tuple variant's payload is.
///
/// `bounds(0, 64)` cannot go through [`walk_keys`]: a bare literal is not valid `Meta` at all, so
/// there is no key to dispatch on. Arity is checked here because a tuple's arity is fixed by its
/// declaration, not by what was written.
pub fn positional(body: ListBody<'_>, arity: usize, what: &str) -> Result<Vec<syn::Expr>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{ListBody, Opening};
    use syn::{Attribute, ItemStruct, parse_str};

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

    const KEYS: &[&str] = &["colour", "name", "retry"];

    #[test]
    fn every_key_is_dispatched() {
        let seen = with_body("#[c(colour(Red), name = \"x\", retry(times = 1))]", |body| {
            let mut seen = Vec::new();
            walk_keys(body, KEYS, |key, _| {
                seen.push(key.to_owned());
                Ok(())
            })
            .expect("all keys known");
            seen
        });

        assert_eq!(seen, ["colour", "name", "retry"]);
    }

    #[test]
    fn an_unknown_key_carries_the_candidates_and_the_walk_continues() {
        let (error, seen) = with_body("#[c(colur(Red), name = \"x\")]", |body| {
            let mut seen = Vec::new();
            let result = walk_keys(body, KEYS, |key, _| {
                seen.push(key.to_owned());
                Ok(())
            });
            (result.err().expect("colur is unknown"), seen)
        });

        assert_eq!(error.to_string(), "expected one of: `colour`, `name`, `retry`");
        // the sibling after the bad element still ran
        assert_eq!(seen, ["name"]);
    }

    #[test]
    fn a_duplicate_key_is_reported_once_and_the_first_wins() {
        let (error, seen) = with_body("#[c(name = \"a\", name = \"b\")]", |body| {
            let mut seen = Vec::new();
            let result = walk_keys(body, KEYS, |key, _| {
                seen.push(key.to_owned());
                Ok(())
            });
            (result.err().expect("written twice"), seen)
        });

        assert!(error.to_string().contains("more than once"), "{error}");
        assert_eq!(seen, ["name"], "the second is not dispatched");
    }

    #[test]
    fn no_sibling_is_lost_when_several_fail() {
        // THE property. Three bad elements, three complaints - contrast a `?`-based Parse impl,
        // which reports one and drops the rest.
        let error = with_body("#[c(one(x), two(y), three(z))]", |body| {
            walk_keys(body, KEYS, |_, _| Ok(())).err().expect("all unknown")
        });

        assert_eq!(error.into_iter().count(), 3);
    }

    #[test]
    fn an_error_from_the_callback_is_kept_too() {
        let error = with_body("#[c(colour(Red), name = \"x\")]", |body| {
            walk_keys(body, KEYS, |key, meta| {
                if key == "colour" {
                    Err(Error::new_spanned(meta, "colour is unhappy"))
                } else {
                    Ok(())
                }
            })
            .err()
            .expect("the callback failed")
        });

        assert!(error.to_string().contains("colour is unhappy"), "{error}");
    }

    #[test]
    fn a_key_matches_on_its_last_segment() {
        // ID(walk/suffix)
        let seen = with_body("#[c(some::colour(Red))]", |body| {
            let mut seen = Vec::new();
            walk_keys(body, KEYS, |key, _| {
                seen.push(key.to_owned());
                Ok(())
            })
            .expect("last segment matches");
            seen
        });

        assert_eq!(seen, ["colour"]);
    }

    #[test]
    fn a_body_that_is_not_a_meta_list_aborts() {
        // Nothing to salvage - there are no siblings to lose.
        let result = with_body("#[c(0, 64)]", |body| walk_keys(body, KEYS, |_, _| Ok(())));
        assert!(result.is_err());
    }

    #[test]
    fn positional_checks_arity_against_the_declaration() {
        with_body("#[c(0, 64)]", |body| {
            assert_eq!(positional(body, 2, "Bounds").unwrap().len(), 2);

            let error = positional(body, 1, "Other").err().expect("wrong arity");
            assert_eq!(error.to_string(), "`Other` takes 1 argument, 2 supplied");
        });
    }
}
