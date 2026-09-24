// @review [ ]
//! The render walk: one pass over a finished extraction tree, emitting every reason it finds.
//!
//! Closes ID(syntax/render). Until this existed only the ROOT's reasons were emitted - children
//! recorded theirs faithfully and nobody ever read them, which made `#no-result`'s guarantee half
//! a promise: no sibling was dropped, but no sibling was reported either.
//!
// NOTE(#render/who-renders): V[Impl(Diagnose).for(Extracted).has(renders reasons)], "Extracted renders its own reasons; the walk finds children"
// The split is deliberate. `Extracted` renders a node's OWN reasons, because it is the only thing
// that knows the node they span against - a reason carries a Span only when it has something finer
// to point at, and falls back to the node otherwise (ID(reason/span-not-node)). A VALUE implements
// Diagnose only to say where its children are. So a grammar type never has to know how a reason
// becomes an error, and the fallback can never be forgotten
//!
// NOTE(#render/traversal-is-source-order): V[F(render) != sorts], "Errors come out in traversal order, which is source order"
// ID(syntax/render) asked for the errors to be SORTED BY SPAN. That is not possible and does not
// need to be. VERIFIED: ordering spans requires Span::start(), which is gated on proc-macro2's
// `span-locations` feature, and inside a real proc macro the compiler branch returns LineColumn {
// line: 0, column: 0 } - every span compares equal, so a sort would be a no-op that looked like a
// guarantee. It is also unnecessary: the walk is depth-first over a syn tree that was built in
// source order, so the errors come out in source order already. The requirement was satisfied by
// the traversal rather than by a comparator

use syn::Error;

// TODO[x](#assert/rules-ride-the-walk): U[Tr(Diagnose)], "Assert is Diagnose's supertrait, so rules ride the one walk"
// Tr(Assert) becomes a SUPERTRAIT. A rule stated three levels down must reach the top, spanned
// against its own node. F(render) already descends every child and already knows that node, so
// making Tr(Assert) a supertrait gets propagation with no second traversal - see
// ID(assert/diagnose-requires-assert) for what it costs

use crate::assert::Assert;
use crate::extractor::{Extracted, Reason, ReasonKind};

/// Say where a node's children are, so the walk can reach them.
///
/// Implementors do NOT render their own reasons - the `Extracted` wrapping them does that, because
/// it holds the node those reasons span against.
///
/// NOTE(#assert/diagnose-requires-assert): V[Tr(Diagnose).impl(Assert)], "Diagnose requires Assert, so every walked type can be asked"
/// Tr(Assert) is a SUPERTRAIT, so a type that can be diagnosed can always be asked what rules it
/// breaks - even when the answer is none. That is what lets the rules ride this walk instead of
/// needing one of their own: F(render) already descends every child and already knows the node each
/// reason spans against, so a rule stated three levels down arrives correctly placed for free.
///
/// THE COST, recorded rather than discovered later: every Tr(Diagnose) implementor now needs an
/// `impl Assert for X {}` as well. The method is defaulted so that is one line
/// (ID(assert/default-is-empty)), but it is a line an author hand-writing an extraction type has
/// to write, and forgetting it is `the trait bound X: Assert is not satisfied`. Paid deliberately:
/// a second walk would have duplicated this one, and two traversals of the same tree drift.
///
/// NOTE(#diagnose/values-are-walkable): V[Impl(Diagnose).has(grammar, leaf, vocab)], "Every askable value is walkable, with an empty walk"
/// Every askable VALUE is walkable too, with an empty walk - it holds no S(Extracted), so there is
/// nothing beneath it to reach, and saying so is the true answer rather than a stub.
///
/// Without it a grammar-typed field could not be walked, so Attr(derive(Diagnose)) refused it
/// with `Column: Diagnose is not satisfied` and the author reached for Attr(skip) - which compiled,
/// and silently dropped every rule the grammar stated. That was ID(diagnose-derive/walk-all-and-
/// skip)'s cheap mistake turned into its expensive one, and it happened: the demo's
/// `conflicts(skip, key)` never fired. Walkable values are what put Attr(skip) back on the fields
/// that genuinely have nothing to say, like a borrowed Ident.
///
/// Not a defaulted method: a hand-written walk that FORGOT its children would then compile, and
/// that is the silent failure this trait is shaped to prevent.
pub trait Diagnose: Assert {
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

/// DELIBERATELY EMPTY, and the emptiness is the design rather than a stub.
///
/// NOTE(#assert/extracted-is-the-handoff): V[Impl(Assert).for(Extracted).is(empty)], "Assert on Extracted is empty so each rule reports once"
/// The two walks cover different ground and meet exactly here. Tr(Assert) descends WITHIN a value -
/// into the grammar nodes and collections a value holds. Tr(Diagnose) descends ACROSS S(Extracted)
/// boundaries, and asks each value it reaches for its rules on the way past.
///
/// So an S(Extracted) reached during an ASSERT walk must not descend, or its value's rules are
/// reported twice: once by the parent's assert walk and once when the diagnose walk arrives at it
/// independently. Making this empty is what keeps every rule reported exactly once, and it is the
/// kind of thing that would otherwise be found as a duplicated diagnostic long after.
impl<T, I> Assert for Extracted<T, I> {}

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
            // THE ONE PLACE a rule becomes an error, and it is here for the same reason a reason
            // is (ID(render/who-renders)): this is what holds the node to span against. A rule
            // is about a value, so there is nothing to check when extraction produced none.
            let mut violations = Vec::new();
            value.assert(&mut violations);
            for violation in violations {
                out.push(violation.to_error(self.source(), violation.kind.message()));
            }

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

// Matches Tr(Assert)'s forwarding set, so a grammar holding `Box<Nested>` is walkable wherever it
// is askable - ID(diagnose/values-are-walkable).
impl<T: Diagnose + ?Sized> Diagnose for Box<T> {
    fn diagnose(&self, out: &mut Vec<Error>) {
        (**self).diagnose(out);
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
            // Worded HERE, from the rule and the keys it names, rather than at the site that
            // recorded it - ID(reason/message).
            ReasonKind::Violated(violation) => violation.message(),
            // The held error's own wording. Only the FIRST of a combined error appears here -
            // the rest survive in the error itself, which F(to_error) returns whole.
            ReasonKind::Internal(error) => format!("internal: {error}"),
            ReasonKind::Syntax(error) => error.to_string(),
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

    use crate::assert::Assert;

    impl Assert for Parent {}
    impl Assert for Child {}

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

    // ---- ReasonKind carrying a syn::Error -------------------------------------------------

    fn combined_error() -> Error {
        let mut first = Error::new(proc_macro2::Span::call_site(), "first complaint");
        first.combine(Error::new(proc_macro2::Span::call_site(), "second complaint"));
        first
    }

    #[test]
    fn a_carried_error_is_not_flattened() {
        // THE property that justifies the kind holding a syn::Error rather than a String.
        // `Error::combine` keeps each sub-error's own span; rebuilding from `.to_string()` would
        // collapse both into one message at one span. See ID(reason/error-is-carried-not-rebuilt).
        let tree = Extracted::new(
            Extraction::value(Parent {
                children: vec![child(
                    "a",
                    vec![Reason::new(ReasonKind::Syntax(combined_error()))],
                )],
            }),
            ident("root"),
        );

        let errors = tree.render();
        assert_eq!(errors.len(), 1, "one reason");
        // ...but that ONE reason still carries BOTH complaints
        assert_eq!(errors[0].clone().into_iter().count(), 2);
        assert_eq!(errors[0].to_compile_error().to_string().matches("compile_error").count(), 2);
    }

    #[test]
    fn a_reasons_own_span_still_wins_over_a_carried_error() {
        // ID(reason/span-not-node)'s precedence, now that there are two possible span sources.
        let token = ident("colur");
        let reason = Reason::at(ReasonKind::Syntax(combined_error()), &token);

        assert!(reason.span().is_some(), "the finer pointer is kept");
    }

    #[test]
    fn internal_is_distinguishable_from_the_authors_fault() {
        // The one bit fail-upward is gated on - ID(reason/fault-is-declared).
        let ours = Reason::new(ReasonKind::Internal(combined_error()));
        let theirs = Reason::new(ReasonKind::Syntax(combined_error()));

        assert!(ours.is_internal());
        assert!(!theirs.is_internal());
        assert!(!Reason::new(ReasonKind::WrongShape).is_internal());
    }

    #[test]
    fn an_internal_reason_says_so_in_its_wording() {
        let ours = Reason::new(ReasonKind::Internal(Error::new(
            proc_macro2::Span::call_site(),
            "assembled badly",
        )));
        assert!(ours.message().starts_with("internal:"), "{}", ours.message());
    }

    #[test]
    fn or_span_adopts_only_when_there_is_nothing_finer() {
        let token = ident("finer");
        let coarse = ident("coarse");

        // nothing of its own -> adopts
        let adopted = Reason::new(ReasonKind::Missing).or_span(coarse.span());
        assert!(adopted.span().is_some());

        // something finer -> keeps it
        let kept = Reason::at(ReasonKind::Missing, &token).or_span(coarse.span());
        assert_eq!(
            format!("{:?}", kept.span().unwrap()),
            format!("{:?}", token.span()),
            "a parent must not overwrite a child that already knew",
        );
    }

    // ---- an author's own reason tree ------------------------------------------------------

    /// What a grammar author writes: an exhaustive enum of their own, and the meaning of each.
    enum ColourReason {
        NotAColour { written: String },
        TooManyChannels,
    }

    impl crate::extractor::AuthorReason for ColourReason {
        fn message(&self) -> String {
            match self {
                // exhaustive, in the AUTHOR's code - which is the point of them having a type
                ColourReason::NotAColour { written } => {
                    format!("`{written}` is not a colour")
                }
                ColourReason::TooManyChannels => "a colour takes three channels".to_owned(),
            }
        }
    }

    #[test]
    fn an_author_reason_renders_through_the_framework() {
        let reason = Reason::custom(ColourReason::NotAColour {
            written: "chartreuse".into(),
        });

        assert_eq!(reason.message(), "`chartreuse` is not a colour");
        // the author said nothing about position, so the framework's fallback owns it
        assert!(reason.span().is_none());
    }

    #[test]
    fn an_author_reason_falls_back_to_the_node_it_sits_on() {
        // ID(reason/span-not-node), reached without the author having to know it exists.
        let tree = Extracted::new(
            Extraction::value(Parent {
                children: vec![child("a", vec![Reason::custom(ColourReason::TooManyChannels)])],
            }),
            ident("root"),
        );

        let errors = tree.render();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].to_string(), "a colour takes three channels");
    }

    #[test]
    fn an_author_may_still_point_at_one_token() {
        struct Precise(proc_macro2::Span);
        impl crate::extractor::AuthorReason for Precise {
            fn message(&self) -> String {
                "here".to_owned()
            }
            fn span(&self) -> Option<proc_macro2::Span> {
                Some(self.0)
            }
        }

        let token = ident("chartreuse");
        let reason = Reason::custom(Precise(token.span()));
        assert!(reason.span().is_some(), "an author with something finer keeps it");
    }
    /// A value that breaks a rule, so the collection point can be exercised.
    struct Ruled(bool);

    impl Assert for Ruled {
        fn assert(&self, out: &mut Vec<Reason>) {
            if self.0 {
                out.push(Reason::new(ReasonKind::Violated(
                    crate::assert::Violation::new(
                        crate::assert::AssertKind::OneOf,
                        &["times", "forever"],
                    ),
                )));
            }
        }
    }

    impl Diagnose for Ruled {
        // a leaf: no children to visit
        fn diagnose(&self, _: &mut Vec<Error>) {}
    }

    #[test]
    fn a_rule_a_value_breaks_reaches_the_walk() {
        // ID(assert/a-rule-is-a-reason): no second traversal was added - the rule rides the walk
        // that already collects reasons, and arrives worded from the rule rather than from a
        // string baked when it was recorded.
        let node = quote!(retry(times = 3, forever = true));
        let extracted = Extracted::new(Extraction::value(Ruled(true)), node);

        let errors = extracted.render();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].to_string(),
            "expected exactly one of `times`, `forever`"
        );
    }

    #[test]
    fn a_satisfied_rule_adds_nothing_to_the_walk() {
        let node = quote!(retry(times = 3));
        let extracted = Extracted::new(Extraction::value(Ruled(false)), node);

        assert!(extracted.render().is_empty());
    }

    #[test]
    fn a_broken_rule_does_not_suppress_the_value() {
        // Non-fatal, which is ID(generator/stub-is-not-empty) seen from this end: the extraction
        // still HAS its value, so generation still runs and the author still gets their item. A
        // rule that voided the value would turn one bad key into an error at every use site.
        let node = quote!(retry(times = 3, forever = true));
        let extracted = Extracted::new(Extraction::value(Ruled(true)), node);

        assert!(!extracted.render().is_empty());
        assert!(extracted.value().is_some(), "a broken rule threw the value away");
    }

    #[test]
    fn a_rule_is_not_asked_of_an_extraction_that_produced_nothing() {
        // There is no value to have rules about. The node's own reasons are still taken.
        let node = quote!(retry());
        let extracted: Extracted<Ruled, _> =
            Extracted::new(Extraction::failed(Reason::new(ReasonKind::Missing)), node);

        let errors = extracted.render();
        assert_eq!(errors.len(), 1, "{errors:?}", errors = errors.len());
        assert_eq!(errors[0].to_string(), "required, and not written");
    }

}
