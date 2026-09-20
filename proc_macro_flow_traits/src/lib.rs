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
 * which is shorter, needs no inference, and closed Fix[x](#from/names-its-target) on the way.
 *
 * The second gain is that a RULE can become a CONTRACT. Tr(Generator)'s stub-always rule used to be
 * four lines every entry point had to write correctly; F(stub) is now a required method and F(emit)
 * the only way to spend it, so the rule cannot be forgotten - see
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
 * TODO[x](#deps): C[root.has(T(syn))] && C[root.has(T(proc_macro2))], "This crate has NO dependencies today, so it cannot name syn::Meta, syn::Expr, syn::Error or proc_macro2::Span - and every item below is defined in terms of those. Add syn (parsing + printing, and `full` to match the derive crate) and proc-macro2 before anything else here is written, or each task silently becomes unimplementable in place"
 *
 * TODO[ ](#home): C[N(syntax)], "One module for the whole surface, so `mod syntax;` sits beside extractor/processor/generator rather than the items scattering through lib.rs. Every task in this block currently names the crate root only because that module does not exist yet - move them into it as it lands"
 *
 * --- THE TRAIT SURFACE -----------------------------------------------------
 *
 * Answer(#traits):A[ID(traits) == Tr(Shape) + Tr(FromMeta)], "SUPERSEDED, not built, and the
 * substitution is worth recording because the original is still the better-sounding design. It
 * asked for ONE TRAIT PER SHAPE - FromPath, FromMetaList, FromNameValue - so that Attr(shape)
 * lowers to a trait BOUND rather than a runtime match, and a type never declared parsable in a
 * given shape fails in the AUTHOR's crate. That goal was met; the decomposition was not.
 *
 * What shipped is Tr(Shape) - `type Input<'ast>` plus `const KIND` - with the three shapes as
 * ZSTs, and ONE Tr(FromMeta) that reads a node out of whichever variant it was written as. The
 * reason for the swap is ID(openings): syn::Meta already HAS exactly three variants, so three
 * traits would have been a second three-valued vocabulary sitting beside rustc's own, free to
 * disagree with it. Tr(Shape) names the variant instead of duplicating the choice.
 *
 * The bound still exists and still fails in the author's crate - it is `T: FromMeta` plus the
 * shape's Ty(Input) - so nothing about ID(no-runtime-shape-match) was given up. See
 * NOTE(#leaves/uniform-field-read) for why there is deliberately no blanket
 * `impl<T: FromExpr> FromMeta for T`"
 *
 * TODO[x](#leaves): C[Tr(FromExpr).F(from_expr).A(\1).T(&Expr)], "DONE, in vocab::leaves.
 * Tr(FromExpr) reads a terminal out of a value position, with impls for Ident, Path, Expr, bool
 * and the six Lit* types via M(leaf). The reasoning held exactly: Meta cannot represent bare
 * literals, so `sizes(1, 2)` needs Expr underneath, and Meta::List::tokens being raw is what
 * makes that reachable.
 *
 * ONE NAME ON ITS LIST WAS A MISTAKE. It asked for `Type` among the terminals. VERIFIED that Type
 * is not a grammar leaf at all: the only place it appears is as the payload of Attr(shape), and
 * that is resolved by S(Unresolved)::resolve through syn::parse2 - syn's own Tr(Parse) - never
 * through Tr(FromExpr). A value position holds a VALUE; the shape selector holds a TYPE, and they
 * are different readings. No impl is owed"
 *
 * TODO[x](#bool-double-duty): V[Impl(bool).impl(FromExpr)] && V[M(flag).covers(presence)], "DONE,
 * and the assertion it exists to protect is now written at the impl itself (vocab/leaves.rs): bool
 * reads a LITERAL through Tr(FromExpr), while its other reading - presence as a flag - belongs to
 * M(flag). The two coexist deliberately and shape selection resolves which applies.
 *
 * The trait NAMES in the original are stale - it said FromPath and FromNameValue, which
 * Answer(#traits) replaced - but the substance is unchanged and is exactly the thing nobody should
 * later 'fix' as an apparent conflict"
 *
 * TODO[ ](#forwarding): C[Impl(Option<T>).impl(FromMetaList)] && C[Impl(Vec<T>).impl(FromMetaList)] && C[Impl(Box<T>).impl(FromMetaList)], "Adapters for Option<T>, Vec<T>, NonEmpty<T>, Punctuated<T, Sep>, Box<T> and Spanned<T>. This is where requiredness and arity are enforced, which keeps 'how many' in exactly one place - the field type - instead of smeared across the shape traits. Box<T> is what makes a recursive grammar terminate; Spanned<T> is the opt-in span boundary that lets every other grammar type stay plain data. Sep is contingent on ID(syntax/separator)"
 *
 * --- ERRORS AS DATA --------------------------------------------------------
 *
 * TODO[~](#reason): C[E(Reason).V(WrongShape)] && C[E(Reason).V(UnknownKey)] && C[E(Reason).V(Missing)] && C[E(Reason).V(Duplicate)] && C[E(Reason).V(Ambiguous)] && C[E(Reason).V(Custom)], "CLOSED REASONS: DONE. OPEN RENDERING: HALF DONE.
 * The variant set landed in extractor::ReasonKind and gained Duplicate, which this list omitted -
 * six, not five. Authors still never construct a message, so they cannot produce an unspanned or
 * context-free one, which was the point.
 *
 * The RENDERING half is where it stands open. E(ReasonKind)::message() gives each variant its
 * default wording and is called LATE, at render time, which is the property that matters - see
 * NOTE(#render/who-renders). What is still owed is the other half of 'render it against Node':
 * there is no Node table yet, so a message cannot say WHICH node it is about or what that node
 * would have accepted. Blocked on ID(node-table), and ID(diagnostics) is blocked on both.
 *
 * ID(extractor/error) is fully closed against this - a per-type error taxonomy bought nothing and
 * could not combine with a sibling's. ID(syntax/custom-reason) is still unanswered and is overdue:
 * it said 'decide before ID(reason) is written', and ID(reason) is now mostly written"
 *
 * TODO[x](#node-table): C[S(Node).P(name)] && C[S(Node).P(aliases)] && C[S(Node).P(shapes)] && C[S(Node).P(children)], "The reflection const each derive emits. This one table pays for 'expected one of ..', 'did you mean ..' and 'colour is a list here, not a name-value'. It is what makes STRICT MATCHING, LENIENT SUGGESTIONS possible - resolution stays case-sensitive while the did-you-mean search is not, so leniency sits in diagnostics where a wrong guess is free rather than in resolution where it costs a canonical form"
 *
 * TODO[ ](#diagnostics): C[Tr(Diagnostics).F(message).R(String)], "Author-overridable RENDERING, blanket default provided. Scoped to rephrasing and never to construction: the framework keeps the span and the tree position, so the worst an author can do is bad prose in the right place. The case that earns it is domain vocabulary - a DSL wants 'unknown column option', which the framework cannot know and which should not cost the author spans or did-you-mean to obtain"
 *
 * --- PARSING ENTRY POINTS --------------------------------------------------
 *
 * TODO[ ](#entry): C[F(from_body).A(\1).T(TokenStream)] && C[F(from_attributes).A(\1).T(&[Attribute])] && C[F(from_args).A(\1).T(TokenStream)], "from_body does the work; the other two are thin adapters. Every attribute-bearing syn node exposes .attrs, so &[Attribute] is the universal entry and POSITION (item / field / variant) never needs modelling at all. A proc_macro_attribute hands its args over already unwrapped, so that path is less work, not different work. Document the one real asymmetry: empty args have no span, and #[a] is indistinguishable from #[a()] there, so a bare-flag grammar ROOT works under a derive only"
 *
 * TODO[ ](#resolve): C[F(resolve).R(Extraction<Self>)], "Type-directed: gather the expected type's candidates, match exactly, accept any SUFFIX of a canonical path, allow a ZST field to be written as key OR value, then zero matches -> 'not accepted here, expected one of ..' and several -> 'ambiguous, qualify'. Suffix matching is free for every node and needs nothing declared, and mirroring rustc's own import semantics means the rule is one users already hold. Keys are idents and values are paths, exactly the asymmetry Rust has in `Foo { bar: Baz::Qux }` - fields are not items, so there is no `configuration::colour` to resolve and the qualified key form is dropped"
 *
 * TODO[x](#render): C[F(render).R(Vec<syn::Error>)] && V[F(render).contains(compile_error)], "DONE, in proc_macro_flow_traits::render, and the signature landed as Vec<syn::Error> rather than TokenStream: the walk COLLECTS, and F(emit_errors) turns the collection into a stream. Splitting them is what lets the entry point interleave the walk's errors with the processor's before anything is emitted. VERIFIED as specified: syn::Error::combine keeps each error's own span and to_compile_error emits one compile_error! per error, so all-at-once reporting needs no nightly diagnostics. The stub goes out ALONGSIDE the errors, which was the other half - see ID(generator/stub-is-not-empty). TWO CLAUSES DID NOT SURVIVE CONTACT, both recorded rather than quietly dropped: SORTED BY SPAN is impossible AND unnecessary (ID(render/traversal-is-source-order)), and DEDUPE AND CAP are simply not done (ID(render/dedupe-and-cap)). The premise behind both - 'traversal order is not source order' - was wrong for the tree as built: depth-first over a syn tree assembled in source order IS source order. It will stop being wrong the moment ID(resolve) reports a missing-required key discovered after the written ones, which is exactly when ID(render/dedupe-and-cap) becomes due"
 *
 * TODO[ ](#render/dedupe-and-cap): C[F(render).dedupes] && C[F(render).caps], "ID(render) asked the single final pass to sort, dedupe and cap. It sorts by construction and does NEITHER of the other two. Not done rather than deemed unnecessary: a grammar that rejects one malformed node N ways produces N errors today, and nothing bounds the count, so one bad attribute can bury a screen. Deferred because both need a key to compare on that does not exist yet - dedupe needs reason-plus-span equality, which needs ID(reason) to settle what a rendered message IS, and a cap needs an ordering to decide what to drop first, which is the one thing spans cannot give on stable. Do it when ID(reason)'s reflection table lands, not before: capping on an arbitrary order would hide errors at random"
 *
 * TODO[ ](#testing): C[F(parse_grammar).A(\1).T(&str)], "Parse a &str into an Attribute and run a grammar against it, so grammar tests need no macro invocation at all - which is only possible because this crate is an ordinary lib. Plus trybuild snapshots of the messages: they are GENERATED from Reason x Node rather than written by hand, which makes them exactly the output worth pinning, since a regression there is otherwise silent"
 */
