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
* NOTE(#traits): the three attribute shapes are ONE Tr(Shape) (`type Input` + `const KIND`) plus one
* Tr(FromMeta), not three traits. syn::Meta already has exactly three variants, so a second
* three-valued vocabulary beside rustc's own could only disagree with it.
* NOTE(#node-table): Ty(Node) is a VIEW DERIVED FROM Tr(Keys), never a second declaration - so it
* cannot advertise a key the walker would reject. Nothing resolves against a Ty(Node); it decides
* only how a failure reads.
