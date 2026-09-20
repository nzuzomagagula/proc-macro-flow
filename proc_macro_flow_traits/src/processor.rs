// @review [ ]
//! The processing stage: making an extraction MEAN something.
//!
//! Extraction walks to nodes and carries them; this is where what they say gets understood. A
//! processor is **optional** - most extractions copy their fields through and generate from them
//! with no transformation at all, which is what `#[derive(Processor)]` emits.
//!
//! NOTE(#processor/receives-whole): V[Ty(Input) == Ty(Extractor::Output)], "A processor takes the
//! extractor's Output AS HANDED OVER - `Extracted<T, I>`, not the `Extraction` inside it. The
//! pipeline does not unwrap between stages, because unwrapping strips the source node off exactly
//! the value that needs it: a FAILED extraction has no T to ask, and that is the case a diagnostic
//! needs position for most"
//!
//! NOTE(#processor/output-needs-no-bound): V[Tr(Extractor).Ty(Output).unbounded], "This answers
//! ID(extractor/output-bound), and the answer is that no bound is needed. The obvious move once
//! Processor exists is to bound Extractor::Output as 'something a processor can consume' - but an
//! extractor does not know which processor will consume it, and nothing makes it one-to-one. The
//! relationship is declared from the OTHER side: a Processor names its Input, and that is where the
//! two are required to agree. Bounding Output would assert a coupling that does not exist"
//!
//! NOTE(#processor/cascade-is-a-helper): V[F(process_each) != Tr(Processor).A(children)], "Children
//! are processed BEFORE their parent combines them, but that is a discipline the helpers support
//! rather than a shape the signature enforces. For the framework to hand a parent its children
//! already processed, the parent's Input would have to be its own extraction with every child field
//! replaced by that child's Output - a type-level transformation of a struct, which Rust cannot
//! express. So `process_each` mirrors `extract_each` and the parent calls it first. Same bottom-up
//! order, no machinery that does not exist"

/* @group(#processor/fluid)
 *
 * What a processor is FOR, recorded before the derive is touched. Processing is the stage that
 * cannot be abstracted: extraction is a walk and generation is a splice, but understanding what was
 * written is the author's business by definition. The items below follow from that.
 *
 * TODO[ ](#processor/derive-enforces):U[MacDef(Processor)], "Attr(derive(Processor)) currently
 * emits a working IDENTITY impl, which is the OPPOSITE of the job it should do. The derive should
 * ENFORCE that the logic exists, not supply it: derive the PLUMBING - Ty(Input) and Ty(Output)
 * wired from Attr(source) - and delegate the body to an inherent `process_impl` the author must
 * write, so a missing body is a compile error rather than a silently trivial pipeline.
 *
 * This SUPERSEDES a decision, so the conflict is recorded rather than quietly dropped: this
 * module's header and ID(processor/optionality) both currently justify the identity on the grounds
 * that 'downstream sees one shape and never an either-or'. That argument is still true - it is why
 * ID(pipeline/no-processor-is-the-extractor) has no two-stage variant - but it is an argument for
 * the identity being AVAILABLE, not for it being what the derive hands you by default. Decide
 * whether the identity survives under another name"
 *
 * NOTE(#processor/holds-state): V[S(Processor).P ??], "A processor is a STRUCT, and the intent is
 * that it may CARRY STATE across its children - counting items, collecting every struct definition
 * in a module, accumulating something the children contribute to one at a time. Tr(Processor)::
 * process is STATIC (`fn process(input) -> ..`), so today it cannot: there is no `self` to
 * accumulate into. Recorded as a signature question rather than pre-empted, because `&self` and
 * `&mut self` are different designs - the first lets a configured processor be reused, the second
 * makes the cascade order load-bearing in a way ID(processor/cascade-is-a-helper) currently leaves
 * free. Decide it when a real stateful processor exists to test it against"
 *
 * TODO[ ](#processor/generalises-over-traits):C[Impl(Processor).over(Tr)], "A processor may define
 * ITS OWN traits, which extractors then implement, and be implemented OVER THAT TRAIT rather than
 * over a concrete extraction type: `impl<T: MyInput> Processor for MyProc<T>`, calling methods
 * instead of matching types. That is how one processor handles several shapes of input without
 * knowing any of them - the processing-side twin of @group(#multi-source), and the same answer:
 * the generic goes on the TYPE, and what varies is reached through a trait rather than inferred.
 * Cross-reference both; they should be decided together, because a processor generic over an input
 * trait and an extractor generic over its source are the same mechanism seen from two ends"
 */

use crate::extractor::Extraction;

/// Turn an extraction into whatever the generator wants to consume.
///
/// It validates nothing: by the time an extraction arrives its reasons are already recorded, and
/// re-checking would duplicate a test it cannot improve on while discarding the spans that make the
/// result diagnosable. TRANSFORM only.
pub trait Processor: Sized {
    /// An extractor's `Output`, whole.
    type Input;

    /// What the generator receives.
    type Output;

    fn process(input: Self::Input) -> Extraction<Self::Output>;

    /// Process many children. Mirrors `Extractor::extract_each`, and is how a parent collects its
    /// children's OUTPUTS before combining - the cascade is bottom-up.
    fn process_each<N>(inputs: N) -> Vec<Extraction<Self::Output>>
    where
        N: IntoIterator<Item = Self::Input>,
    {
        inputs.into_iter().map(Self::process).collect()
    }

    /// A child that may not be there. Absence is not a failure - the `Option` in the field's type
    /// is what says so.
    fn process_maybe(input: Option<Self::Input>) -> Option<Extraction<Self::Output>> {
        input.map(Self::process)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::{Extracted, Extraction};
    use std::cell::RefCell;

    // A child that records WHEN it ran, so the cascade's order is observable rather than assumed.
    thread_local! {
        static ORDER: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    }

    struct Child(u8);
    struct ChildProcessed(u8);

    impl Processor for Child {
        type Input = Extracted<Child, ()>;
        type Output = ChildProcessed;

        fn process(input: Self::Input) -> Extraction<Self::Output> {
            ORDER.with(|o| o.borrow_mut().push("child"));
            match input.into_extraction().value {
                Some(child) => Extraction::value(ChildProcessed(child.0 * 2)),
                None => Extraction::default(),
            }
        }
    }

    struct Parent;
    struct ParentProcessed(u8);

    impl Processor for Parent {
        type Input = Vec<Extracted<Child, ()>>;
        type Output = ParentProcessed;

        fn process(input: Self::Input) -> Extraction<Self::Output> {
            // Children FIRST, then combine - the whole point of ID(processor/cascade-is-a-helper).
            let children = Child::process_each(input);
            ORDER.with(|o| o.borrow_mut().push("parent"));

            let total = children
                .into_iter()
                .filter_map(|c| c.value)
                .map(|c| c.0)
                .sum();

            Extraction::value(ParentProcessed(total))
        }
    }

    fn extracted(n: u8) -> Extracted<Child, ()> {
        Extracted::new(Extraction::value(Child(n)), ())
    }

    #[test]
    fn the_cascade_runs_bottom_up() {
        ORDER.with(|o| o.borrow_mut().clear());

        let out = Parent::process(vec![extracted(1), extracted(2)]);

        // every child before the parent, and the parent saw their OUTPUTS (doubled), not inputs
        ORDER.with(|o| assert_eq!(*o.borrow(), ["child", "child", "parent"]));
        assert_eq!(out.value.unwrap().0, 6);
    }

    #[test]
    fn process_each_keeps_one_result_per_child() {
        let out = Child::process_each(vec![extracted(3), extracted(4)]);
        let values: Vec<_> = out
            .into_iter()
            .filter_map(|c| c.value)
            .map(|c| c.0)
            .collect();
        assert_eq!(values, [6, 8]);
    }

    #[test]
    fn a_child_that_produced_nothing_still_occupies_its_place() {
        // Absence is not silence: the slot survives so position in the tree is not lost.
        let empty: Extracted<Child, ()> = Extracted::new(Extraction::default(), ());
        let out = Child::process_each(vec![extracted(1), empty]);

        assert_eq!(out.len(), 2);
        assert!(out[1].value.is_none());
    }

    #[test]
    fn process_maybe_passes_absence_through() {
        assert!(Child::process_maybe(None).is_none());
        assert!(Child::process_maybe(Some(extracted(5))).is_some());
    }
}
