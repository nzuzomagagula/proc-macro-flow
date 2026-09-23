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
 * TODO[x](#node-table): C[S(Node).P(name)] && C[S(Node).P(aliases)] && C[S(Node).P(shapes)] && C[S(Node).P(children)], "DONE, in proc_macro_flow_traits::node, with all four fields - name, aliases, shapes, children - plus ARITY, which this item did not ask for and which turns out to be the thing that makes it useful: the framework can answer `what is missing` without the caller restating requiredness.
 *
 * The shape it landed in is the one NOTE(#keys/one-table) argues for: Ty(Node) is a VIEW DERIVED FROM Tr(Keys), not a second declaration, so it cannot advertise a key the walker would reject. It replaced M(meta_list)'s `const KEYS: &[&str]`, which was exactly the rival list this collapse removes. Nothing resolves against a Ty(Node) - resolution is Tr(Keys)::resolve and only that - which is why the strings in it do not contradict ID(type-backed).
 *
 * ID(reason)'s open half and ID(diagnostics) were both blocked on this and are now merely unwritten"
 *
 * TODO[ ](#diagnostics): C[Tr(Diagnostics).F(message).R(String)], "Author-overridable RENDERING, blanket default provided. Scoped to rephrasing and never to construction: the framework keeps the span and the tree position, so the worst an author can do is bad prose in the right place. The case that earns it is domain vocabulary - a DSL wants 'unknown column option', which the framework cannot know and which should not cost the author spans or did-you-mean to obtain"
 *
 * --- PARSING ENTRY POINTS --------------------------------------------------
 *
 * TODO[ ](#entry): C[F(from_body).A(\1).T(TokenStream)] && C[F(from_attributes).A(\1).T(&[Attribute])] && C[F(from_args).A(\1).T(TokenStream)], "from_body does the work; the other two are thin adapters. Every attribute-bearing syn node exposes .attrs, so &[Attribute] is the universal entry and POSITION (item / field / variant) never needs modelling at all. A proc_macro_attribute hands its args over already unwrapped, so that path is less work, not different work. Document the one real asymmetry: empty args have no span, and #[a] is indistinguishable from #[a()] there, so a bare-flag grammar ROOT works under a derive only"
 *
 * TODO[ ](#resolve): C[F(resolve).R(Extraction<Self>)], "Type-directed: gather the expected type's candidates, match exactly, accept any SUFFIX of a canonical path, allow a ZST field to be written as key OR value, then zero matches -> 'not accepted here, expected one of ..' and several -> 'ambiguous, qualify'. Suffix matching is free for every node and needs nothing declared, and mirroring rustc's own import semantics means the rule is one users already hold. Keys are idents and values are paths, exactly the asymmetry Rust has in `Foo { bar: Baz::Qux }` - fields are not items, so there is no `configuration::colour` to resolve and the qualified key form is dropped"
 *
 */
