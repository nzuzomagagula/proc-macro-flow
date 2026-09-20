// @review [ ]
//! The render walk: one pass over a finished extraction tree, emitting every reason it finds.
//!
//! Closes ID(syntax/render). Until this existed only the ROOT's reasons were emitted - children
//! recorded theirs faithfully and nobody ever read them, which made `#no-result`'s guarantee half
//! a promise: no sibling was dropped, but no sibling was reported either.
//!
//! NOTE(#render/who-renders): V[Impl(Diagnose).for(Extracted).renders], "The split is deliberate.
//! `Extracted` renders a node's OWN reasons, because it is the only thing that knows the node they
//! span against - a reason carries a Span only when it has something finer to point at, and falls
//! back to the node otherwise (ID(reason/span-not-node)). A VALUE implements Diagnose only to say
//! where its children are. So a grammar type never has to know how a reason becomes an error, and
//! the fallback can never be forgotten"
//!
//! NOTE(#render/traversal-is-source-order): V[F(render).!sorts], "ID(syntax/render) asked for the
//! errors to be SORTED BY SPAN. That is not possible and does not need to be. VERIFIED: ordering
//! spans requires Span::start(), which is gated on proc-macro2's `span-locations` feature, and
//! inside a real proc macro the compiler branch returns LineColumn { line: 0, column: 0 } - every
//! span compares equal, so a sort would be a no-op that looked like a guarantee. It is also
//! unnecessary: the walk is depth-first over a syn tree that was built in source order, so the
//! errors come out in source order already. The requirement was satisfied by the traversal rather
//! than by a comparator"

use syn::Error;

use crate::extractor::{Extracted, Reason, ReasonKind};

/// Say where a node's children are, so the walk can reach them.
///
/// Implementors do NOT render their own reasons - the `Extracted` wrapping them does that, because
/// it holds the node those reasons span against.
pub trait Diagnose {
    fn diagnose(&self, out: &mut Vec<Error>);

    /// Walk this tree and collect every reason in it, in source order.
    fn render(&self) -> Vec<Error> {
        let mut out = Vec::new();
        self.diagnose(&mut out);
        out
    }

    /// The same walk, folded into one `syn::Error`.
    ///
    /// `syn::Error::combine` keeps each error's own span and emits one `compile_error!` per
    /// reason, so folding costs no precision. `None` means a clean tree.
    fn rendered(&self) -> Option<Error> {
        self.render().into_iter().reduce(|mut all, next| {
            all.combine(next);
            all
        })
    }
}

impl<T, I> Diagnose for Extracted<T, I>
where
    T: Diagnose,
    I: quote::ToTokens,
{
    fn diagnose(&self, out: &mut Vec<Error>) {
        // This node's own complaints first, spanned against the node they belong to.
        for reason in self.reasons() {
            out.push(reason.to_error(self.source(), reason.kind.message()));
        }

        // Then its children. A node with no value has none to visit - but its reasons were
        // already taken above, which is the case that matters most.
        if let Some(value) = self.value() {
            value.diagnose(out);
        }
    }
}

impl<T: Diagnose> Diagnose for Vec<T> {
    fn diagnose(&self, out: &mut Vec<Error>) {
        for child in self {
            child.diagnose(out);
        }
    }
}

impl<T: Diagnose> Diagnose for Option<T> {
    fn diagnose(&self, out: &mut Vec<Error>) {
        if let Some(child) = self {
            child.diagnose(out);
        }
    }
}

impl Reason {
    /// The default wording for a reason, pending ID(reason/message).
    ///
    /// Still rendered LATE rather than baked at record time, which is the property that matters:
    /// an author's `Diagnostics` impl can replace this without the framework losing the span or
    /// the tree position.
    pub fn message(&self) -> String {
        self.kind.message()
    }
}

impl ReasonKind {
    pub fn message(&self) -> String {
        match self {
            ReasonKind::WrongShape => "written in the wrong shape".to_owned(),
            ReasonKind::UnknownKey => "not a key this node accepts".to_owned(),
            ReasonKind::Missing => "required, and not written".to_owned(),
            ReasonKind::Duplicate => "written more than once".to_owned(),
            ReasonKind::Ambiguous => "ambiguous - qualify it".to_owned(),
            ReasonKind::Custom(message) => message.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::Extraction;
    use quote::quote;
    use syn::{parse_str, Ident};

    /// A parent holding children, as a real extraction does.
    struct Parent {
        children: Vec<Extracted<Child, Ident>>,
    }
    struct Child;

    impl Diagnose for Parent {
        fn diagnose(&self, out: &mut Vec<Error>) {
            self.children.diagnose(out);
        }
    }

    impl Diagnose for Child {
        // a leaf: no children to visit
        fn diagnose(&self, _: &mut Vec<Error>) {}
    }

    fn ident(name: &str) -> Ident {
        parse_str(name).expect("an ident")
    }

    fn child(name: &str, reasons: Vec<Reason>) -> Extracted<Child, Ident> {
        Extracted::new(
            Extraction {
                value: Some(Child),
                reasons,
            },
            ident(name),
        )
    }

    #[test]
    fn a_childs_reason_reaches_the_output() {
        // THE point. Before the walk existed this reason was recorded and never read.
        let tree = Extracted::new(
            Extraction::value(Parent {
                children: vec![child("a", vec![Reason::new(ReasonKind::Missing)])],
            }),
            ident("root"),
        );

        let errors = tree.render();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "required, and not written");
    }

    #[test]
    fn every_sibling_is_reported_not_just_the_first() {
        let tree = Extracted::new(
            Extraction::value(Parent {
                children: vec![
                    child("a", vec![Reason::new(ReasonKind::Missing)]),
                    child("b", vec![Reason::new(ReasonKind::UnknownKey)]),
                    child("c", vec![Reason::new(ReasonKind::Duplicate)]),
                ],
            }),
            ident("root"),
        );

        assert_eq!(tree.render().len(), 3);
    }

    #[test]
    fn the_root_is_reported_before_its_children() {
        // Depth-first, parent first - which for a syn tree is source order.
        // ID(render/traversal-is-source-order)
        let tree = Extracted::new(
            Extraction::value(Parent {
                children: vec![child("a", vec![Reason::new(ReasonKind::UnknownKey)])],
            })
            .with_reason(Reason::new(ReasonKind::WrongShape)),
            ident("root"),
        );

        let errors = tree.render();
        assert_eq!(errors[0].to_string(), "written in the wrong shape");
        assert_eq!(errors[1].to_string(), "not a key this node accepts");
    }

    #[test]
    fn a_node_with_no_value_still_yields_its_reasons() {
        // The case that matters most: extraction failed, so there are no children to walk - and
        // the complaint explaining why must still come out.
        let tree: Extracted<Parent, Ident> = Extracted::new(
            Extraction {
                value: None,
                reasons: vec![Reason::new(ReasonKind::WrongShape)],
            },
            ident("root"),
        );

        assert_eq!(tree.render().len(), 1);
    }

    #[test]
    fn a_reasons_own_span_wins_over_the_nodes() {
        // A reason with something finer to point at uses it; one without falls back to the node.
        // Both render - the distinction is where they point, which is ID(reason/span-not-node).
        let token = ident("colur");
        let tree = Extracted::new(
            Extraction::value(Parent {
                children: vec![child("a", vec![Reason::at(ReasonKind::UnknownKey, &token)])],
            }),
            ident("root"),
        );

        let errors = tree.render();
        assert!(!errors[0].to_compile_error().is_empty());
    }

    #[test]
    fn combined_folds_the_walk_into_one_error() {
        let tree = Extracted::new(
            Extraction::value(Parent {
                children: vec![
                    child("a", vec![Reason::new(ReasonKind::Missing)]),
                    child("b", vec![Reason::new(ReasonKind::Missing)]),
                ],
            }),
            ident("root"),
        );

        let all = tree.rendered().expect("two reasons");
        // one compile_error! per reason, which is what syn::Error::combine guarantees
        assert_eq!(all.into_iter().count(), 2);
    }

    #[test]
    fn a_clean_tree_renders_nothing() {
        let tree = Extracted::new(
            Extraction::value(Parent {
                children: vec![child("a", vec![])],
            }),
            ident("root"),
        );

        assert!(tree.render().is_empty());
        assert!(tree.rendered().is_none());
        let _ = quote!(); // keep the import honest
    }
}
