// @review [ ]
//! The reflection table each grammar node emits, and the one thing diagnostics render against.
//!
//! NOTE(#keys/one-table): V[S(Node).derived_from(Tr(Keys))], "ID(node-table) and the key set are
//! the SAME STRUCTURE seen twice, so this is a VIEW over Tr(Keys) rather than a second declaration.
//! The direction is what matters: a Ty(Node) is built from the names that actually match, so it
//! cannot describe a key the walker would reject or omit one it accepts. Two independent lists
//! could, and that is the drift ID(type-backed) is aimed at.
//!
//! WHY STRINGS ARE CORRECT HERE, given NOTE(#keys/table-is-strings-the-rest-is-not) says to keep
//! them out of everything else: this is the one place a string is the OUTPUT. Ty(Node) exists to
//! render `expected one of: a, b, c`, and a message is text. Nothing RESOLVES against a Ty(Node) -
//! resolution is Tr(Keys)::resolve and only that - so no spelling here can decide whether something
//! matches. It can only decide how a failure reads"

use crate::meta::ShapeKind;

/// How many times a child may be written, read off the field's TYPE.
///
/// The same three-way split ID(from/arity-from-type) uses for extraction, for the same reason:
/// nothing may contradict what the type already says, so there is no `#[required]` attribute.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arity {
    /// `T` — must be written exactly once.
    Required,
    /// `Option<T>` — absence is not a failure.
    Optional,
    /// `Vec<T>` — any number, including none.
    Repeated,
}

/// One child of a grammar node.
#[derive(Clone, Copy, Debug)]
pub struct Child {
    /// The canonical spelling. Aliases are accepted on the way in and never produced.
    pub key: &'static str,
    /// Extra accepted spellings, canonical excluded.
    pub aliases: &'static [&'static str],
    pub arity: Arity,
    /// The shapes this child may be written in. Empty means every shape its type implements.
    pub shapes: &'static [ShapeKind],
}

/// A grammar node, as the framework can see it.
#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub name: &'static str,
    pub children: &'static [Child],
}

impl Node {
    /// Every accepted spelling of every child, canonical and alias alike.
    pub fn spellings(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.children
            .iter()
            .flat_map(|child| std::iter::once(child.key).chain(child.aliases.iter().copied()))
    }

    /// The did-you-mean list, in `lookahead1`'s shape.
    pub fn candidates(&self) -> String {
        let quoted: Vec<String> = self
            .spellings()
            .map(|spelling| format!("`{spelling}`"))
            .collect();
        quoted.join(", ")
    }

    /// The child written under a spelling, canonical or alias.
    pub fn child(&self, spelling: &str) -> Option<&Child> {
        self.children
            .iter()
            .find(|child| child.key == spelling || child.aliases.contains(&spelling))
    }

    /// Children that must be written and were not.
    ///
    /// The framework can answer this because arity is in the table; the caller supplies only what
    /// it saw. That is what ID(reason)'s Missing variant has been waiting for.
    pub fn missing<'a>(&'a self, written: &'a [&str]) -> impl Iterator<Item = &'a Child> + 'a {
        self.children.iter().filter(move |child| {
            child.arity == Arity::Required && !written.contains(&child.key)
        })
    }
}

/// A type that can describe itself.
///
/// TODO[x](#node-table/derive-emits-it): DONE. Attr(derive(Syntax)) emits this for every grammar
/// type, alongside M(meta_list)'s hand-written impl - so ID(diagnostics) and the Node half of
/// ID(reason) are unblocked and merely unwritten. What is NOT yet emitted is a child's `shapes`
/// where no Attr(shape) was written: the field stays empty, meaning "every shape its type
/// implements", which is ID(shape-attr)'s absent-is-additive rule and not a gap.
pub trait Described {
    const NODE: Node;
}

#[cfg(test)]
mod tests {
    use super::*;

    const RETRY: Node = Node {
        name: "retry",
        children: &[
            Child {
                key: "times",
                aliases: &[],
                arity: Arity::Required,
                shapes: &[ShapeKind::NameValue],
            },
            Child {
                key: "backoff",
                aliases: &["delay"],
                arity: Arity::Optional,
                shapes: &[],
            },
        ],
    };

    #[test]
    fn candidates_include_aliases() {
        let list = RETRY.candidates();
        assert!(list.contains("`times`"), "{list}");
        assert!(list.contains("`backoff`"), "{list}");
        assert!(list.contains("`delay`"), "{list}");
    }

    #[test]
    fn a_child_is_found_by_canonical_or_alias() {
        assert_eq!(RETRY.child("backoff").unwrap().key, "backoff");
        // an alias finds the same child, and reports the CANONICAL name
        assert_eq!(RETRY.child("delay").unwrap().key, "backoff");
        assert!(RETRY.child("nope").is_none());
    }

    #[test]
    fn missing_reports_only_required_children() {
        let missing: Vec<&str> = RETRY.missing(&[]).map(|child| child.key).collect();
        assert_eq!(missing, ["times"], "an optional child is never missing");

        assert_eq!(RETRY.missing(&["times"]).count(), 0);
    }
}
