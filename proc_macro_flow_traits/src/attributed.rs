// @review [ ]
//! What an ATTRIBUTE macro was handed: its own arguments, and the item they were written on.
//!
// NOTE(#attributed/source-is-a-pair): V[Tr(Validate).Ty(Source) ==? S(Attributed)], "An attribute macro's Source is a pair: args and item"
// A derive has ONE input and an attribute macro has TWO, and until this type existed the second was
// simply dropped - the generated entry read `pub fn trace(_attr: TokenStream, input: TokenStream)`
// and `#[trace(level = "debug")]` could not be written at all.
//!
// Answer(#pipeline/subject-equals-source-breaks-attributes): A[ID(pipeline/subject-equals-source-breaks-attributes) ==? S(Attributed)], "Misdiagnosed: the Source was one node, not the binding"
// the limitation was misdiagnosed and
// there is nothing to work around.
//!
//! It read `Generator::Subject = Validate::Source` as the obstacle and concluded an attribute
//! macro could not be a pipeline, so the pipeline macro drove its own stages by hand. The binding
//! was never the problem. The problem was that Ty(Source) was a SINGLE borrowed node, and an
//! attribute macro's source is a PAIR. Making the pair a node - Copy, Visitable, ToTokens, like
//! any other - leaves the binding exactly as it was and the special case disappears: the pipeline
//! macro now runs through F(run) like everything else (ID(pipeline-macro/is-a-pipeline)).
//!
//! Worth keeping as a caution rather than deleting. The original note was well argued and wrong,
//! and it stood long enough to shape ~60 lines of bespoke orchestration around it"

// TODO[x](#attribute/source-is-a-pair): C[S(Attributed)] && C[Tr(Annotated)], "Attributed carries both inputs of an attribute macro"
// An attribute macro has TWO inputs and Ty(Source) was one borrowed node, so the arguments were
// discarded - the generated entry read `_attr: TokenStream`. A pair that is itself a node carries
// both, and Tr(Annotated) makes the macro KIND a fact the compiler checks rather than a promise
// E(MacroKind) makes

use quote::ToTokens;
use syn::visit::Visit;

use crate::visitable::Visitable;

/// The two halves of an attribute macro's invocation.
///
/// Two references, so `Copy` is free and `Tr(Pipeline)::run`'s `Source: Copy` bound is met without
/// anything being cloned. Nothing here owns an AST node, per ID(no-owned-nodes).
pub struct Attributed<'ast, A, I> {
    /// The attribute's own arguments, ALREADY READ into the grammar type that declared them.
    ///
    /// Not a token stream: `args = TraceArgs` names an ordinary `#[derive(Syntax)]` grammar, and
    /// the entry function reads it with the same reader a helper attribute goes through. See
    /// ID(attributed/args-are-a-grammar).
    args: &'ast A,
    /// The item the attribute was written on.
    item: &'ast I,
}

// Derived Copy/Clone would demand `A: Copy, I: Copy`, which is wrong twice over: the fields are
// REFERENCES, so copying one copies a pointer, and no grammar type is Copy. Written out instead.
impl<A, I> Clone for Attributed<'_, A, I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A, I> Copy for Attributed<'_, A, I> {}

impl<'ast, A, I> Attributed<'ast, A, I> {
    pub fn new(args: &'ast A, item: &'ast I) -> Self {
        Self { args, item }
    }

    /// What was written inside the attribute's delimiters.
    pub fn args(self) -> &'ast A {
        self.args
    }

    /// What the attribute was written on.
    pub fn item(self) -> &'ast I {
        self.item
    }
}

/// A source that carries an item an attribute macro must hand BACK.
///
/// NOTE(#attributed/annotated-decides-the-kind): V[Impl(Annotated).for(Attributed)], "Annotated makes the macro kind a fact the compiler checks"
/// This trait is what makes a macro's KIND a fact the compiler checks rather than a promise
/// E(MacroKind) makes. `&'ast DeriveInput` has no impl, so F(run_attribute) is UNCALLABLE on a
/// derive pipeline - and equally uncallable on an attribute pipeline whose extractor forgot to
/// declare `args`, because its Source is then still a bare node.
///
/// Before this the kind was a string in an attribute that nothing could check, and getting it
/// wrong produced a macro that silently discarded half its input. ID(type-backed) is exactly the
/// rule that says a mistake like that should not be expressible
pub trait Annotated<'ast>: Copy {
    /// The half that goes back out - never the arguments, which were consumed reading them.
    type Item: ToTokens;

    fn item(self) -> &'ast Self::Item;
}

impl<'ast, A, I: ToTokens> Annotated<'ast> for Attributed<'ast, A, I> {
    type Item = I;

    fn item(self) -> &'ast I {
        self.item
    }
}

/// Emits the arguments and then the item.
///
/// Used for ONE thing: the fallback node a reason with no finer span of its own is rendered
/// against (ID(reason/span-not-node)). It is deliberately NOT how an attribute macro re-emits its
/// item - that is F(run_attribute), which takes the item alone through Tr(Annotated), so the
/// arguments can never be echoed back into the author's crate.
impl<A: ToTokens, I: ToTokens> ToTokens for Attributed<'_, A, I> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.args.to_tokens(tokens);
        self.item.to_tokens(tokens);
    }
}

/// Visiting the pair visits THE ITEM, and the omission is the honest answer rather than a gap.
///
/// NOTE(#attributed/only-the-item-is-visitable): V[Impl(Visitable).for(Attributed) != bounds(A)], "Visiting the pair visits the item; args are a grammar value"
///
/// The first draft required `&'ast A: Visitable<'ast>` and visited both halves. That does not
/// compile for any real pipeline, and finding out why corrected the design: the args are a GRAMMAR
/// VALUE, not a syn node. `TraceArgs` is the author's own struct, already read out of the tokens
/// by Tr(FromBody), so there is no `visit_trace_args` and there never will be.
///
/// Tr(Visitable) exists so a stage can drive a `syn::Visit` over its source (it is 'how do we get
/// there', the second of the two questions an extractor answers). By the time a pipeline holds an
/// S(Attributed) the arguments have already been got to; what is left to walk is the item. So the
/// bound is on `I` alone, and an args type is free to be any shape its author likes
impl<'ast, A, I> Visitable<'ast> for Attributed<'ast, A, I>
where
    &'ast I: Visitable<'ast>,
{
    fn accept<V: Visit<'ast> + ?Sized>(self, visitor: &mut V) {
        self.item.accept(visitor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::{parse_str, ItemFn, Meta};

    fn item() -> ItemFn {
        parse_str("fn traced() {}").expect("the item parses")
    }

    #[test]
    fn the_pair_is_copy_so_a_pipeline_can_take_it_by_value() {
        // Tr(Pipeline)::run bounds `Source: Copy` - it hands the node to the extractor AND keeps it
        // for the generator's stub. A pair that had to be cloned could not be a Source.
        let args: Meta = parse_str("trace(level = \"debug\")").expect("the meta parses");
        let item = item();
        let node = Attributed::new(&args, &item);

        let taken = node;
        assert_eq!(taken.item().sig.ident, "traced");
        // `node` is still usable: if this were a move the line would not compile.
        assert_eq!(node.item().sig.ident, "traced");
    }

    #[test]
    fn only_the_item_goes_back_out() {
        // ID(attributed/annotated-decides-the-kind). An attribute macro REPLACES what it annotates,
        // so re-emitting the arguments too would paste `level = "debug"` into the author's crate as
        // if it were code.
        let args: Meta = parse_str("trace(level = \"debug\")").expect("the meta parses");
        let item = item();
        let node = Attributed::new(&args, &item);

        let out = Annotated::item(node).to_token_stream().to_string();
        assert!(out.contains("fn traced"), "{out}");
        assert!(!out.contains("level"), "the arguments were re-emitted: {out}");
    }

    #[test]
    fn the_whole_pair_is_still_spannable() {
        // ToTokens exists for the fallback in ID(reason/span-not-node) and nothing else, so it
        // carries BOTH halves - a complaint with nothing finer to point at is about the invocation,
        // which is both.
        let args: Meta = parse_str("trace(level = \"debug\")").expect("the meta parses");
        let item = item();
        let out = Attributed::new(&args, &item).to_token_stream().to_string();

        assert!(out.contains("level"), "{out}");
        assert!(out.contains("fn traced"), "{out}");
    }

    #[test]
    fn a_bare_node_is_not_annotated() {
        // The negative half of ID(attributed/annotated-decides-the-kind), asserted the only way a
        // missing impl can be: this compiles, and the same call on `&DeriveInput` does not.
        fn only_for_annotated<'ast, T: Annotated<'ast>>(_: T) {}

        let args = quote!(level = "debug");
        let item = item();
        only_for_annotated(Attributed::new(&args, &item));
    }
}
