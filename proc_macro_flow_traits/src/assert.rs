// @review [ ]
//! Rules a grammar states about itself, and what it means to break one.
//!
// NOTE(#assert/a-rule-is-a-reason): V[Tr(Assert).R(Reason)], "An assert produces a Reason, which is what makes propagation free"
// An assert produces a E(Reason), and that one decision is what makes propagation free. A E(Reason)
// already knows how to be rendered against the node it sits on (ID(reason/span-not-node)), already
// accumulates rather than returning early (ID(no-result)), and is already collected from every
// level of the tree by the render walk. So a rule declared three levels down needs NO new machinery
// to reach the top - it is carried by the same walk that carries an unknown key.
//
// The alternative, a second traversal invoked beside F(render), would have duplicated a walk that
// already works. See ID(assert/diagnose-requires-assert) for what the shortcut costs.

// TODO[x](#assert/rules-are-declared): C[Tr(Assert)] && C[Tr(Rule)] && C[S(Violation)] && C[E(AssertKind)], "A grammar can state rules like exactly one of these two"
//
// A grammar can state a key's shape, arity and requiredness off the field's type, and cannot say
// 'exactly one of these two' - the commonest real constraint there is. A rule produces a E(Reason),
// which is what makes it cost nothing to propagate

use crate::extractor::Reason;

crate::vocabulary! {
    /// The rules a grammar may state, and the only heads `#[assert(..)]` accepts.
    ///
    /// Closed, and matched from the written head of `one_of(..)` - which is exactly what
    /// M(vocabulary) exists for, as opposed to M(names). Extending it is a deliberate act here,
    /// not something an author does by writing a word we did not expect.
    pub enum AssertKind {
        /// Exactly one of these was written.
        OneOf = "one_of",
        /// At least one of these was written.
        AnyOf = "any_of",
        /// No more than one of these was written.
        AtMostOne = "at_most_one",
        /// `requires(a, b)` - if `a` was written then `b` must be too.
        Requires = "requires",
        /// These may not be written together.
        Conflicts = "conflicts",
        /// The escape hatch: an author's own Tr(Rule).
        With = "with",
    }
}

/// A rule that was broken, named rather than worded.
///
/// NOTE(#assert/violation-is-not-a-message): V[S(Violation) != P(String)], "A Violation holds the rule and fields; wording comes later"
/// It holds the RULE and the FIELDS, and the wording is produced later by F(message). That is
/// ID(reason/message)'s requirement - a message baked at record time is out of reach of anything
/// that might want to rephrase it - and it is also what keeps E(ReasonKind) interpretable: a caller
/// can ask WHICH rule failed and act on the answer, which a string would not allow.
///
/// Both fields are `&'static`, because the derive emits them as literals. Nothing here is built at
/// runtime from anything the author wrote
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Violation {
    pub rule: AssertKind,
    /// The keys the rule was written about, in the order the author named them.
    pub fields: &'static [&'static str],
}

impl Violation {
    pub fn new(rule: AssertKind, fields: &'static [&'static str]) -> Self {
        Self { rule, fields }
    }

    /// The complaint, rendered late from the rule and the keys it names.
    pub fn message(&self) -> String {
        let keys: Vec<String> = self
            .fields
            .iter()
            .map(|field| format!("`{field}`"))
            .collect();
        let keys = keys.join(", ");

        match self.rule {
            AssertKind::OneOf => format!("expected exactly one of {keys}"),
            AssertKind::AnyOf => format!("expected at least one of {keys}"),
            AssertKind::AtMostOne => format!("expected at most one of {keys}"),
            AssertKind::Conflicts => format!("these cannot be written together: {keys}"),
            AssertKind::Requires => match self.fields {
                [first, second] => {
                    format!("`{first}` was written, so `{second}` is required too")
                }
                // The derive rejects any other arity at the author's span, so this is unreachable
                // from a compiled grammar - worded rather than panicked on all the same.
                _ => format!("a dependency between {keys} was not satisfied"),
            },
            // NOTE(#assert/with-never-violates): V[E(AssertKind).V(With) != in(Violation)], "With is a spelling #[assert] accepts, never a Violation kind"
            // An author's rule words its OWN reason through Tr(AuthorReason), so it raises a
            // E(ReasonKind)::Custom and never reaches here. `With` is in this vocabulary because it
            // is a spelling `#[assert(..)]` accepts, not because it is a failure we can describe -
            // the asymmetry is real and is recorded rather than smoothed over.
            AssertKind::With => "a rule on this node was not satisfied".to_owned(),
        }
    }
}

/// What rules this value breaks.
///
/// NOTE(#assert/no-result): V[F(assert).A(out) && F(assert) != R(Result)], "assert accumulates into a buffer and never returns early"
/// Accumulates into a buffer rather than returning, for the reason ID(no-result) gives: with no `?`
/// there is no early return, and with no early return a type stating three rules cannot report the
/// first and drop the other two. Same shape as Tr(Diagnose)::diagnose, deliberately - they are one
/// walk.
///
/// NOTE(#assert/default-is-empty): V[F(assert).is(defaulted)], "assert is defaulted because forgetting it costs nothing"
/// DEFAULTED, which is the opposite of what ID(generator/stub-is-a-contract) decided for F(stub),
/// and the difference is what forgetting costs. A forgotten stub is a missing impl that cascades at
/// every use site, so it is worth making unforgettable. A type with no rules is the overwhelmingly
/// common case and an empty body is the correct answer for it, so requiring one would be ceremony
/// that teaches nothing.
pub trait Assert {
    fn assert(&self, out: &mut Vec<Reason>) {
        let _ = out;
    }
}

/// An author's own rule, written once and applied wherever it is named.
///
/// NOTE(#assert/rule-is-checked-against-its-subject): V[Tr(Rule).Ty(Subject)], "Rule's Subject is associated, so a misplaced rule fails to compile"
/// Ty(Subject) is an associated type, so `#[assert(with = NoZeroRetries)]` on the wrong grammar is
/// a COMPILE ERROR rather than a rule that quietly never fires. Same argument as
/// ID(pipeline/source-is-associated): the type pins it, so no call site has to be trusted to get it
/// right.
pub trait Rule {
    type Subject: ?Sized;

    fn check(subject: &Self::Subject, out: &mut Vec<Reason>);
}

// The forwarding impls, matching the ones Tr(Diagnose) already has in `render`. Without these a
// grammar holding `Option<Nested>` or `Vec<Nested>` would silently not descend, which is exactly
// the propagation this whole trait exists to provide.
impl<T: Assert> Assert for Option<T> {
    fn assert(&self, out: &mut Vec<Reason>) {
        if let Some(value) = self {
            value.assert(out);
        }
    }
}

impl<T: Assert> Assert for Vec<T> {
    fn assert(&self, out: &mut Vec<Reason>) {
        for value in self {
            value.assert(out);
        }
    }
}

impl<T: Assert + ?Sized> Assert for Box<T> {
    fn assert(&self, out: &mut Vec<Reason>) {
        (**self).assert(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::ReasonKind;

    struct Retry {
        times: Option<u8>,
        forever: Option<bool>,
    }

    impl Assert for Retry {
        fn assert(&self, out: &mut Vec<Reason>) {
            let written = [self.times.is_some(), self.forever.is_some()];
            if written.iter().filter(|w| **w).count() != 1 {
                out.push(Reason::new(ReasonKind::Violated(Violation::new(
                    AssertKind::OneOf,
                    &["times", "forever"],
                ))));
            }
        }
    }

    fn violations(value: &impl Assert) -> Vec<Reason> {
        let mut out = Vec::new();
        value.assert(&mut out);
        out
    }

    #[test]
    fn a_satisfied_rule_says_nothing() {
        let value = Retry {
            times: Some(3),
            forever: None,
        };
        assert!(violations(&value).is_empty());
    }

    #[test]
    fn one_of_fires_for_both_and_for_neither() {
        let both = Retry {
            times: Some(3),
            forever: Some(true),
        };
        let neither = Retry {
            times: None,
            forever: None,
        };

        assert_eq!(violations(&both).len(), 1);
        assert_eq!(violations(&neither).len(), 1);
    }

    #[test]
    fn the_message_is_built_from_the_rule_and_its_keys() {
        // ID(assert/violation-is-not-a-message): nothing was worded at record time.
        let violation = Violation::new(AssertKind::OneOf, &["times", "forever"]);
        assert_eq!(violation.message(), "expected exactly one of `times`, `forever`");

        let requires = Violation::new(AssertKind::Requires, &["back_off", "times"]);
        assert_eq!(
            requires.message(),
            "`back_off` was written, so `times` is required too"
        );
    }

    #[test]
    fn a_collection_descends_into_every_element() {
        // THE propagation claim, at its smallest. Without these forwards a nested grammar's rules
        // are simply never run, and nothing would say so.
        let broken = || Retry {
            times: None,
            forever: None,
        };

        assert_eq!(violations(&vec![broken(), broken()]).len(), 2);
        assert_eq!(violations(&Some(broken())).len(), 1);
        assert_eq!(violations(&Box::new(broken())).len(), 1);
        assert!(violations(&Option::<Retry>::None).is_empty());
    }
}
