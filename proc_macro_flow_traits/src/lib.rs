// @review [x]
pub mod extractor;
pub mod generator;
pub mod meta;
pub mod node;
pub mod pipeline;
pub mod processor;
pub mod render;

/* NOTE(#type-backed): V[design.prefers(compile_time)], "THE STANDING HEURISTIC, and it outranks
 * local convenience. Wherever a behaviour can be bound by the TYPE SYSTEM instead of checked at
 * runtime, it is - the point being predictability: a thing the types forbid cannot be got wrong,
 * while a thing a check forbids can be got wrong by anyone who forgets the check.
 *
 * In the two forms it comes up in most:
 *   RUNTIME vs COMPILE TIME -> compile time.
 *   STRING vs ENUM          -> enum.
 *
 * It is already why several decisions went the way they did, which is the evidence it is a real
 * rule and not a preference stated once: ID(pipeline/source-is-associated) made the source an
 * associated type so a call site cannot guess; ID(from/arity-from-type) reads arity off the field
 * TYPE so no attribute can contradict it; ID(generator/stub-is-a-contract) made a remembered rule a
 * required method; ID(typed-output/generate) made generated code a typed item so a malformed one
 * cannot leave the generator; M(names) and M(vocabulary) exist so a name is an ENUM VARIANT with
 * generated conversions rather than a string someone compares.
 *
 * WHERE IT DOES NOT APPLY, and this is the part worth stating so the rule is usable rather than a
 * slogan: user INPUT is runtime by definition. What the author wrote in an attribute is not known
 * until the macro runs, so reading it is always a check. The rule does not ask for that check to be
 * abolished - it asks that the check's RESULT be a type rather than a string, and that everything
 * DOWNSTREAM of it be bound. See NOTE(#shape/two-facts) for the cleanest instance of the split"
 */

/* NOTE(#pipeline/no-free-functions): V[!N(traits).F(free)], "EVERY stage helper is an associated
 * item on the trait it belongs to, and none is a free function. What moved: F(extract)/
 * F(extract_each)/F(extract_maybe) onto Tr(Extractor), F(process_each)/F(process_maybe) onto
 * Tr(Processor), F(render)/F(combined) onto Tr(Diagnose), F(emit)/F(emit_errors) onto
 * Tr(Generator). F(extract) was DELETED outright - it was `T::extract_from(node)` spelled longer.
 *
 * THE REASON IS PROCEDURAL-MACRO ERGONOMICS, not taste. A free function makes the pipeline's
 * parameters implicit in its ARGUMENT LIST, where a macro has to reconstruct them; an associated
 * item makes them the receiver's own, where a macro can read them off a type it already names. The
 * derive shows the difference directly - it used to emit
 * `extract_each::<FieldExtraction, _, _>(it)` and now emits `<FieldExtraction>::extract_each(it)`,
 * NOTE(#generator/stub-is-a-contract). No free function could have done that, because a free
 * function cannot require anything of the type it is handed.
 *
 * COST, recorded honestly: a provided method is only reachable with the trait in scope, so
 * generated code must import it. The derive does that anonymously - `use Tr(Extractor) as _;` -
 * which is the ordinary hygiene bargain it already took for Tr(Validate)"
 */

/// Re-exported so generated code can name `syn` types without the AUTHOR'S crate having to depend
/// on syn under that exact name.
///
/// NOTE(#traits/reexport-syn): V[N(proc_macro_flow_traits)::syn], "Generated code already writes
/// `::proc_macro_flow_traits::..` paths rather than importing, which is the ordinary hygiene
/// bargain: a macro must not depend on what is in scope at the call site. `::syn::Error` in the
/// Tr(Diagnose) body broke that bargain - it compiled only because every crate testing this
/// happens to depend on syn. One re-export closes it for the one syn type generated code names"
pub use syn;

/// Re-exported for the same reason as `syn` — generated code must not assume the AUTHOR'S crate
/// depends on these. `#[derive(Generator)]` emits a `ToTokens` impl, which names both.
pub use proc_macro2;
pub use quote;
pub mod resolution;
pub mod visitable;
pub mod vocab;

/* @group(#syntax)
 *
 * The syntax stage's PUBLIC surface lives in this crate. The design, the scratch and the derive
 * macro itself stay in proc_macro_flow_derive/src/base/syntax/mod.rs - read that header first;
 * these are the items it says must be nameable from outside a proc-macro crate.
 *
 * --- GATES -----------------------------------------------------------------
 *
 * NOTE(#placement): V[root.has(N(syntax))] && V[ID(pipeline/relocate-traits) ==? this], "Step zero, and the same problem #pipeline/relocate-traits already names. VERIFIED: rustc refuses a proc-macro crate that declares ANY pub non-macro item, so the shape traits, Reason, Extraction and Node cannot live in the derive crate - this is a language constraint, not a preference. A PRIVATE grammar type does parse fine there, so expansion-time resolution would work either way; pub in this crate is what puts the grammar in cargo doc and what lets a re-emitted path resolve downstream. Everything below is blocked on this"
 *
 * NOTE(#traits): the three attribute shapes are ONE Tr(Shape) (`type Input` + `const KIND`) plus
 * one Tr(FromMeta), not three traits. syn::Meta already has exactly three variants, so a second
 * three-valued vocabulary beside rustc's own could only disagree with it.
 * NOTE(#leaves/uniform-field-read) for why there is deliberately no blanket
 * `impl<T: FromExpr> FromMeta for T`"
 *
 * NOTE(#node-table): Ty(Node) is a VIEW DERIVED FROM Tr(Keys), never a second declaration, so it
 * cannot advertise a key the walker would reject. Nothing resolves against it; it decides only
 * how a failure reads.
 * TODO[ ](#diagnostics): C[Tr(Diagnostics).F(message).R(String)], "Author-overridable RENDERING, blanket default provided. Scoped to rephrasing and never to construction: the framework keeps the span and the tree position, so the worst an author can do is bad prose in the right place. The case that earns it is domain vocabulary - a DSL wants 'unknown column option', which the framework cannot know and which should not cost the author spans or did-you-mean to obtain"
 *
 * --- PARSING ENTRY POINTS --------------------------------------------------
 *
 * TODO[ ](#entry): C[F(from_body).A(\1).T(TokenStream)] && C[F(from_attributes).A(\1).T(&[Attribute])] && C[F(from_args).A(\1).T(TokenStream)], "from_body does the work; the other two are thin adapters. Every attribute-bearing syn node exposes .attrs, so &[Attribute] is the universal entry and POSITION (item / field / variant) never needs modelling at all. A proc_macro_attribute hands its args over already unwrapped, so that path is less work, not different work. Document the one real asymmetry: empty args have no span, and #[a] is indistinguishable from #[a()] there, so a bare-flag grammar ROOT works under a derive only"
 *
 * TODO[ ](#resolve): C[F(resolve).R(Extraction<Self>)], "Type-directed: gather the expected type's candidates, match exactly, accept any SUFFIX of a canonical path, allow a ZST field to be written as key OR value, then zero matches -> 'not accepted here, expected one of ..' and several -> 'ambiguous, qualify'. Suffix matching is free for every node and needs nothing declared, and mirroring rustc's own import semantics means the rule is one users already hold. Keys are idents and values are paths, exactly the asymmetry Rust has in `Foo { bar: Baz::Qux }` - fields are not items, so there is no `configuration::colour` to resolve and the qualified key form is dropped"
 *
 * TODO[ ](#home): C[N(syntax)], "One module for the whole surface, so `mod syntax;` sits beside extractor/processor/generator rather than the items scattering through lib.rs. Every task in this block currently names the crate root only because that module does not exist yet - move them into it as it lands"
 *
 * --- THE TRAIT SURFACE -----------------------------------------------------
 *
 * TODO[ ](#forwarding): C[Impl(Option<T>).impl(FromMetaList)] && C[Impl(Vec<T>).impl(FromMetaList)] && C[Impl(Box<T>).impl(FromMetaList)], "Adapters for Option<T>, Vec<T>, NonEmpty<T>, Punctuated<T, Sep>, Box<T> and Spanned<T>. This is where requiredness and arity are enforced, which keeps 'how many' in exactly one place - the field type - instead of smeared across the shape traits. Box<T> is what makes a recursive grammar terminate; Spanned<T> is the opt-in span boundary that lets every other grammar type stay plain data. Sep is contingent on ID(syntax/separator)
 *
 * CONSTRAINED since this was written: Option<T> must NEVER get one - see
 * NOTE(#forwarding/no-option) in vocab/leaves.rs. Its absence is what keeps a mis-read arity a
 * COMPILE ERROR rather than a silent change of meaning, and that is VERIFIED, not argued. Vec<T>
 * and Box<T> are unaffected; only Option carries arity."
 *
 * TODO[~](#reason): C[E(Reason).V(WrongShape)] && C[E(Reason).V(UnknownKey)] && C[E(Reason).V(Missing)] && C[E(Reason).V(Duplicate)] && C[E(Reason).V(Ambiguous)] && C[E(Reason).V(Custom)], "CLOSED REASONS: DONE. OPEN RENDERING: HALF DONE.
 * The variant set landed in extractor::ReasonKind and gained Duplicate, which this list omitted -
 * six, not five. Authors still never construct a message, so they cannot produce an unspanned or
 * context-free one, which was the point.
 *
 * The RENDERING half is where it stands open. E(ReasonKind)::message() gives each variant its
 * default wording and is called LATE, at render time, which is the property that matters - see
 * TODO[ ](#render/dedupe-and-cap): C[F(render).dedupes] && C[F(render).caps], "ID(render) asked the single final pass to sort, dedupe and cap. It sorts by construction and does NEITHER of the other two. Not done rather than deemed unnecessary: a grammar that rejects one malformed node N ways produces N errors today, and nothing bounds the count, so one bad attribute can bury a screen. Deferred because both need a key to compare on that does not exist yet - dedupe needs reason-plus-span equality, which needs ID(reason) to settle what a rendered message IS, and a cap needs an ordering to decide what to drop first, which is the one thing spans cannot give on stable. Do it when ID(reason)'s reflection table lands, not before: capping on an arbitrary order would hide errors at random"
 *
 * TODO[ ](#testing): C[F(parse_grammar).A(\1).T(&str)], "Parse a &str into an Attribute and run a grammar against it, so grammar tests need no macro invocation at all - which is only possible because this crate is an ordinary lib. Plus trybuild snapshots of the messages: they are GENERATED from Reason x Node rather than written by hand, which makes them exactly the output worth pinning, since a regression there is otherwise silent"
 */
