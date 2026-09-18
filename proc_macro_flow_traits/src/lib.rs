// @review [x]
pub mod extractor;
pub mod generator;
pub mod meta;
pub mod processor;
pub mod resolution;
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
 * TODO[ ](#traits): C[Tr(FromPath).F(from_path).R(Extraction<Self>)] && C[Tr(FromMetaList).F(from_list).R(Extraction<Self>)] && C[Tr(FromNameValue).F(from_nv).R(Extraction<Self>)], "One trait per attribute shape, so #[shape(..)] lowers to a trait BOUND rather than a runtime match: a type never declared parsable in the selected shape must fail in the AUTHOR's crate at declaration time, which only trait resolution gives. Three and exactly three, because syn::Meta has three variants - Rust's real attribute grammar, not a taxonomy of ours, which is also why it will not drift as the language grows. Closes ID(attribute/list), ID(attribute/path) and ID(attribute/name-value)"
 *
 * TODO[ ](#leaves): C[Tr(FromExpr).F(from_expr).A(\1).T(&Expr)], "Leaf trait for value positions, with impls for the syn terminals (Ident, Path, Type, the Lit* family, Expr) and primitives bridged from literals. Meta cannot represent bare literals, so `sizes(1, 2)` needs Expr underneath, and Meta::List::tokens being raw is what lets a terminal node choose this parser instead. syn::MetaNameValue::value is ALREADY an Expr, so the rhs of `=` costs nothing - half the reason Expr is the leaf grammar"
 *
 * TODO[ ](#bool-double-duty): V[Impl(bool).impl(FromPath)] && V[Impl(bool).impl(FromNameValue)], "bool implements BOTH, deliberately: FromPath is a flag, FromNameValue is a literal. Recorded as an assertion so nobody later 'fixes' the apparent conflict - shape selection resolves it, which is the whole point of a shape being a capability rather than a property"
 *
 * TODO[ ](#forwarding): C[Impl(Option<T>).impl(FromMetaList)] && C[Impl(Vec<T>).impl(FromMetaList)] && C[Impl(Box<T>).impl(FromMetaList)], "Adapters for Option<T>, Vec<T>, NonEmpty<T>, Punctuated<T, Sep>, Box<T> and Spanned<T>. This is where requiredness and arity are enforced, which keeps 'how many' in exactly one place - the field type - instead of smeared across the shape traits. Box<T> is what makes a recursive grammar terminate; Spanned<T> is the opt-in span boundary that lets every other grammar type stay plain data. Sep is contingent on ID(syntax/separator)"
 *
 * --- ERRORS AS DATA --------------------------------------------------------
 *
 * TODO[ ](#reason): C[E(Reason).V(WrongShape)] && C[E(Reason).V(UnknownKey)] && C[E(Reason).V(Missing)] && C[E(Reason).V(Ambiguous)] && C[E(Reason).V(Custom)], "CLOSED REASONS, OPEN RENDERING. Closed so the framework can interpret what it caught and render it against Node; Custom so an exotic grammar is never blocked. Authors never construct a message, so they cannot produce an unspanned or context-free one. Answers ID(extractor/error) structurally: meaning comes from a reason set crossed with a reflection table, never from a taxonomy of error types - a proc macro only ever EMITS an error, so per-type errors buy nothing and actively fight accumulation, since two error structs cannot combine. See ID(syntax/custom-reason)"
 *
 * TODO[ ](#node-table): C[S(Node).P(name)] && C[S(Node).P(aliases)] && C[S(Node).P(shapes)] && C[S(Node).P(children)], "The reflection const each derive emits. This one table pays for 'expected one of ..', 'did you mean ..' and 'colour is a list here, not a name-value'. It is what makes STRICT MATCHING, LENIENT SUGGESTIONS possible - resolution stays case-sensitive while the did-you-mean search is not, so leniency sits in diagnostics where a wrong guess is free rather than in resolution where it costs a canonical form"
 *
 * TODO[ ](#diagnostics): C[Tr(Diagnostics).F(message).R(String)], "Author-overridable RENDERING, blanket default provided. Scoped to rephrasing and never to construction: the framework keeps the span and the tree position, so the worst an author can do is bad prose in the right place. The case that earns it is domain vocabulary - a DSL wants 'unknown column option', which the framework cannot know and which should not cost the author spans or did-you-mean to obtain"
 *
 * --- PARSING ENTRY POINTS --------------------------------------------------
 *
 * TODO[ ](#entry): C[F(from_body).A(\1).T(TokenStream)] && C[F(from_attributes).A(\1).T(&[Attribute])] && C[F(from_args).A(\1).T(TokenStream)], "from_body does the work; the other two are thin adapters. Every attribute-bearing syn node exposes .attrs, so &[Attribute] is the universal entry and POSITION (item / field / variant) never needs modelling at all. A proc_macro_attribute hands its args over already unwrapped, so that path is less work, not different work. Document the one real asymmetry: empty args have no span, and #[a] is indistinguishable from #[a()] there, so a bare-flag grammar ROOT works under a derive only"
 *
 * TODO[ ](#resolve): C[F(resolve).R(Extraction<Self>)], "Type-directed: gather the expected type's candidates, match exactly, accept any SUFFIX of a canonical path, allow a ZST field to be written as key OR value, then zero matches -> 'not accepted here, expected one of ..' and several -> 'ambiguous, qualify'. Suffix matching is free for every node and needs nothing declared, and mirroring rustc's own import semantics means the rule is one users already hold. Keys are idents and values are paths, exactly the asymmetry Rust has in `Foo { bar: Baz::Qux }` - fields are not items, so there is no `configuration::colour` to resolve and the qualified key form is dropped"
 *
 * TODO[ ](#render): C[F(render).R(TokenStream)] && V[F(render).contains(compile_error)], "One walk over the finished tree emitting N spanned compile_error!s, sorted by span. VERIFIED: syn::Error::combine keeps each error's own span and to_compile_error emits one compile_error! per error, so all-at-once reporting needs no nightly diagnostics. A single final pass because traversal order is not source order - written keys are visited before missing-required is discovered - and only one pass can sort, dedupe and cap. Emit a stub expansion ALONGSIDE the errors: without it the missing impl cascades into 'does not implement' at every use site and buries the real diagnostic. Answers half of ID(generator/base-scope)"
 *
 * TODO[ ](#testing): C[F(parse_grammar).A(\1).T(&str)], "Parse a &str into an Attribute and run a grammar against it, so grammar tests need no macro invocation at all - which is only possible because this crate is an ordinary lib. Plus trybuild snapshots of the messages: they are GENERATED from Reason x Node rather than written by hand, which makes them exactly the output worth pinning, since a regression there is otherwise silent"
 */
