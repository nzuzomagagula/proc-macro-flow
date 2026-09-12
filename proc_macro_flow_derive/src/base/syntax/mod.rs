// @review [~]
//
// ===========================================================================
// THE SYNTAX STAGE - attribute grammars declared as ordinary Rust items
// ===========================================================================
//
// Thesis: an attribute grammar IS a set of Rust types. The canonical form of
// every node is the real path to the item that declares it, so parsing is
// *resolution* against those items rather than matching token shapes. This is
// the thing darling cannot do: there the shorthand is the only form, so when it
// is ambiguous you argue with the library. Here the shorthand always desugars
// to a path that can be written out in full.
//
// Why Rust nodes instead of a bespoke grammar:
//   - syn already owns the parsing and the resolution rules; we write none
//   - the grammar tracks the language as it grows, instead of re-matching it
//   - authors and downstream users already know how to read and write paths
//   - "no variant exists" becomes "argument not acceptable" from one reflection
//     table, instead of hand-written string matching at every site
//   - it forecloses managing macros as strings, which is where this gets messy
//
// ---------------------------------------------------------------------------
// SETTLED DESIGN - each decision and what forced it
// ---------------------------------------------------------------------------
//
// GRAMMAR   Meta on the spine, Expr at the leaves. Every *named* grammar node is
//           a syn::Meta - Path / List / NameValue are exactly the three shapes -
//           and nesting stays Meta as deep as there are named nodes.
//           syn::MetaNameValue::value is already an Expr, so the rhs of `=`
//           costs nothing; Meta::List::tokens is raw, so a terminal node parses
//           its payload as Punctuated<Expr, Comma>, which is how `sizes(1, 2)`
//           works even though bare literals are not valid Meta.
//           VERIFIED: `configuration.colour(..)` does NOT parse as Meta
//           ("expected `,`"). `configuration::colour(..)` does, multi-segment
//           path intact. The dotted form is out - and `::` is the more honest
//           spelling anyway, since `.` implies field access on a value while
//           this is a path to an item.
//
// SHAPE     #[shape(AttributeKind::MetaList)] SELECTS; it does not validate.
//           One type may implement several shapes, and the parent narrows which
//           are acceptable at that position. Absent = accept every shape the
//           type implements, and the written Meta variant selects; present =
//           narrow to the listed ones. So the annotation is purely additive and
//           never restates what the type already says.
//           It must lower to a trait bound, never a runtime match: if the type
//           never declared that shape, the AUTHOR's crate fails to compile,
//           which is the only place that error can usefully land.
//           VERIFIED: #[AttributeKind::MetaList] cannot be an attribute HEAD -
//           heads resolve in the macro namespace, where enum variants do not
//           exist, and derive helpers are registered as bare unqualified idents
//           with no path to them. Inside the delimiters rustc does no resolution
//           at all, so the path survives as tokens and can be re-emitted into
//           generated code, where it lands in the value namespace and resolves.
//
// TYPE      The field type carries arity and requiredness, and nothing is
//           permitted to contradict it: T required, Option<T> optional,
//           Vec<T> / NonEmpty<T> repeated, Punctuated<T, Sep> repeated with its
//           separator encoded in the type. No #[required] and no #[default] - a
//           second source of truth that can disagree with the type is precisely
//           the darling failure mode. Absent means None, and the author applies
//           defaults afterwards in ordinary Rust.
//
// NAMES     Exact match; #[alias(..)] for everything else. Shortening is NOT
//           aliasing - a written path matches a node when it is a SUFFIX of that
//           node's canonical path, so `Other` == `ColourSetting::Other` for free
//           with rustc's own ambiguity semantics. Resolution is type-directed,
//           so the candidate set at a position is one enum's variants and
//           collisions are near-impossible.
//           Keys are idents, values are paths - exactly the asymmetry Rust
//           already has in `Foo { bar: Baz::Qux }`. Fields are not items, so
//           there is no `configuration::colour` to resolve, and the qualified
//           key form is dropped. A ZST-valued field may be written as either its
//           key or its value, since a ZST value carries the same information as
//           its key; that is what lets `NoClean` stand for `no_clean` with no
//           case-bridging rule anywhere.
//           Principle: STRICT MATCHING, LENIENT SUGGESTIONS. Resolution is
//           case-sensitive; the did-you-mean search is not. Leniency belongs in
//           diagnostics, where a wrong guess costs nothing, not in resolution,
//           where it costs a canonical form.
//
// ERRORS    Errors are DATA IN THE EXTRACTION TREE, not a side effect of
//           building it. Each node carries its own reasons, so position in the
//           tree IS the diagnostic context: nothing is threaded downward and no
//           separate sink has to be kept in sync with the structure.
//           extract_from returns Extraction<Self>, never Result - there is no
//           `?`, no early return, and therefore no way to drop a sibling or lose
//           a position. The enforcement is structural, not a convention.
//           Reasons carry their own spans, because one node can hold several
//           pointing at different tokens; that also settles absence spans, since
//           whoever records the reason supplies the span.
//           VERIFIED: syn::Error::combine keeps each error's own span, and
//           to_compile_error emits one compile_error! per error - so reporting
//           everything at once works on stable, no nightly diagnostics needed.
//
// PLACEMENT Grammar types belong in a lib crate, not the proc-macro crate.
//           VERIFIED: a `pub` non-macro item in a proc-macro crate is rejected
//           outright ("proc-macro crate types cannot export any items other than
//           functions tagged with #[proc_macro*]"); a PRIVATE one is legal and
//           parses attributes perfectly well. So expansion-time resolution works
//           either way, but `pub` in a companion lib is what puts the grammar in
//           cargo doc and what lets a re-emitted path resolve downstream.
//           Same argument as #pipeline/relocate-traits, same fix.
//
// ---------------------------------------------------------------------------
// THE WORKED EXAMPLE, corrected against every decision above
// ---------------------------------------------------------------------------
//
//   // author's LIB crate (not the proc-macro crate - see PLACEMENT)
//   #[derive(Syntax)]
//   pub struct Configuration {           // entry attribute: `configuration`
//       #[shape(AttributeKind::MetaList)]
//       colour: Vec<ColourSetting>,      // required and many, both from the type
//       name: Option<ConfigName>,        // optional; any shape ConfigName has
//       no_clean: Option<NoClean>,       // flag; presence is the signal
//   }
//   #[derive(Syntax)] pub enum   ColourSetting { Red, Black, Other(Ident) }
//   #[derive(Syntax)] pub struct NoClean;
//   #[derive(Syntax)] pub struct ConfigName(LitStr);
//
//   // downstream user
//   #[configuration(
//       colour(ColourSetting::Red, Other(Blue)),  // suffix match on the value
//       name = "thing",
//       NoClean,                                  // == no_clean
//   )]
//   pub struct Thing;
//
// Note what changed from the scratch below: Vec<..> because arity lives in the
// type; Other(Ident) because `String` has no Expr reading and bare `Blue` is an
// ident; no `configuration::` prefix because keys are idents; Option<NoClean>
// rather than bool so every token still resolves to a real item.
//
// ---------------------------------------------------------------------------
// ORDER OF WORK. #syntax/* is a prerequisite of the extractor stage, not a
// sibling of it: TransformationExtraction collapses into a grammar parse, so
// #attribute/list, #attribute/path and #attribute/name-value stop being three
// hand-written Meta matchers and become three shape traits plus one leaf trait.
// The traits must move to proc_macro_flow_traits FIRST (#pipeline/relocate-
// traits) because a proc-macro crate cannot export them - that is step zero.
// ---------------------------------------------------------------------------

/* @group(#syntax)
 *
 * --- GATES -----------------------------------------------------------------
 *
 * NOTE(#placement): V[N(proc_macro_flow_traits).has(N(syntax))]
 *   && V[ID(pipeline/relocate-traits) ==? this],
 *   "Step zero, and the same problem #pipeline/relocate-traits already names. The shape traits,
 *   Reason, Extraction and Node must live in the ordinary lib crate. VERIFIED: rustc refuses a
 *   proc-macro crate that declares ANY pub non-macro item, so this is a language constraint, not
 *   a preference. A private grammar type does parse fine, so expansion-time resolution works
 *   either way - but pub in a lib is what puts the grammar in cargo doc and what lets a
 *   re-emitted path resolve downstream. Every other #syntax task is blocked on this"
 *
 * NOTE(#no-path-head): V[Attr(shape) != Attr(AttributeKind::MetaList)],
 *   "The selector must stay a SINGLE-SEGMENT helper attribute taking the variant path as an
 *   ARGUMENT. VERIFIED: #[AttributeKind::MetaList] fails with `cannot find type AttributeKind in
 *   this scope` - attribute heads resolve in the macro namespace, enum variants are not in it,
 *   and derive helpers are registered as bare idents with no path to them. Unfixable, not merely
 *   inconvenient. Inside the delimiters rustc resolves nothing, so the path survives as tokens
 *   and can be re-emitted into generated code, where it does resolve"
 *
 * --- THE TRAIT SURFACE -----------------------------------------------------
 *
 * TODO[ ](#traits): C[Tr(FromPath).F(from_path).R(Extraction<Self>)]
 *   && C[Tr(FromMetaList).F(from_list).R(Extraction<Self>)]
 *   && C[Tr(FromNameValue).F(from_nv).R(Extraction<Self>)],
 *   "One trait per attribute shape, so #[shape(..)] lowers to a trait BOUND rather than a runtime
 *   match on AttributeKind: a type never declared parsable in the selected shape must fail in the
 *   AUTHOR's crate at declaration time, which only trait resolution gives. Three and exactly
 *   three, because syn::Meta has three variants - that is Rust's real attribute grammar and not a
 *   taxonomy of ours, which is also why it will not drift as the language grows.
 *   Closes ID(attribute/list), ID(attribute/path) and ID(attribute/name-value)"
 *
 * TODO[ ](#leaves): C[Tr(FromExpr).F(from_expr).A(\1).T(&Expr)],
 *   "Leaf trait for value positions, with impls for the syn terminals (Ident, Path, Type, the Lit*
 *   family, Expr) and primitives bridged from literals. Justification: Meta cannot represent bare
 *   literals, so `sizes(1, 2)` needs Expr underneath, and Meta::List::tokens being raw is exactly
 *   what lets a terminal node choose this parser instead. syn::MetaNameValue::value is ALREADY an
 *   Expr, so the rhs of `=` costs nothing - half the reason Expr is the leaf grammar"
 *
 * TODO[ ](#bool-double-duty): V[Impl(bool).impl(FromPath)] && V[Impl(bool).impl(FromNameValue)],
 *   "bool implements BOTH, deliberately: FromPath is a flag, FromNameValue is a literal. Recorded
 *   as an assertion so nobody later 'fixes' the apparent conflict - shape selection resolves it,
 *   which is the whole point of a shape being a capability rather than a property"
 *
 * TODO[ ](#forwarding): C[Impl(Option<T>).impl(FromMetaList)]
 *   && C[Impl(Vec<T>).impl(FromMetaList)]
 *   && C[Impl(Box<T>).impl(FromMetaList)],
 *   "Adapters for Option<T>, Vec<T>, NonEmpty<T>, Punctuated<T, Sep>, Box<T> and Spanned<T>.
 *   Justification: this is where requiredness and arity are enforced, which keeps 'how many' in
 *   exactly one place - the field type - instead of smeared across the shape traits. Box<T> is
 *   what makes a recursive grammar terminate; Spanned<T> is the opt-in span boundary that lets
 *   every other grammar type stay plain data"
 *
 * --- ERRORS AS DATA --------------------------------------------------------
 *
 * TODO[ ](#extraction): R[E(ExtractionState) -> S(Extraction)]
 *   && C[S(Extraction).P(value).T(Option<T>)]
 *   && C[S(Extraction).P(reasons).T(Vec<Spanned<Reason>>)],
 *   "Justification: Result<ExtractionState<Self>, E> encodes 'did it work' twice, and NEITHER a
 *   two-state enum nor a three-state one can say 'this node extracted fine AND carries a complaint
 *   of its own' - which is exactly what an unknown key is, a failure of the PARENT to consume its
 *   input while its value stays perfectly good. All four combinations are meaningful: Some/[]
 *   clean, Some/[..] partial, None/[..] failed, None/[] absent. Reasons carry their own spans
 *   because one node can hold several pointing at different tokens, which also settles absence
 *   spans. Supersedes the typestate framing in ID(cleanup)"
 *
 * TODO[ ](#no-result): U[Tr(Extractor).F(extract_from).R(Result<ExtractionState<Self>, Self::ExtractionError>) -> R(Extraction<Self>)],
 *   "THE enforcement, and the reason the stage is shaped this way at all. With no Result there is
 *   no `?`, no early return, and no control-flow path that discards a node - so losing a sibling
 *   or a position becomes unrepresentable rather than discouraged by convention. A failed node is
 *   still a node, which is also what lets a later stage see WHICH subtree broke instead of finding
 *   a hole and not knowing why"
 *
 * TODO[ ](#reason): C[E(Reason).V(WrongShape)] && C[E(Reason).V(UnknownKey)]
 *   && C[E(Reason).V(Missing)] && C[E(Reason).V(Ambiguous)] && C[E(Reason).V(Custom)],
 *   "CLOSED REASONS, OPEN RENDERING. Closed so the framework can interpret what it caught and
 *   render it against Node; Custom so an exotic grammar is never blocked. Authors never construct
 *   a message, so they cannot produce an unspanned or context-free one. Answers ID(extractor/error)
 *   structurally: meaning comes from a reason set crossed with a reflection table, never from a
 *   taxonomy of error types - a proc macro only ever EMITS an error, so per-type errors buy
 *   nothing and actively fight accumulation, since two error structs cannot combine"
 *
 * TODO[ ](#node-table): C[S(Node).P(name)] && C[S(Node).P(aliases)]
 *   && C[S(Node).P(shapes)] && C[S(Node).P(children)],
 *   "The reflection const each derive emits. Justification: this one table pays for 'expected one
 *   of ..', 'did you mean ..' and 'colour is a list here, not a name-value'. It is what makes
 *   STRICT MATCHING, LENIENT SUGGESTIONS possible - resolution stays case-sensitive while the
 *   did-you-mean search is not, so leniency sits in diagnostics where a wrong guess is free
 *   rather than in resolution where it costs a canonical form"
 *
 * TODO[ ](#diagnostics): C[Tr(Diagnostics).F(message).R(String)],
 *   "Author-overridable RENDERING, blanket default provided. Scoped to rephrasing and never to
 *   construction: the framework keeps the span and the tree position, so the worst an author can
 *   do is bad prose in the right place. The case that earns it is domain vocabulary - a DSL wants
 *   'unknown column option', which the framework cannot know and which should not cost the author
 *   spans or did-you-mean to obtain"
 *
 * TODO[ ](#render): C[F(render).R(TokenStream)] && V[F(render).contains(compile_error)],
 *   "One walk over the finished tree emitting N spanned compile_error!s, sorted by span.
 *   VERIFIED: syn::Error::combine keeps each error's own span and to_compile_error emits one
 *   compile_error! per error, so all-at-once reporting needs no nightly diagnostics. Justification
 *   for a single final pass: traversal order is not source order (written keys are visited before
 *   missing-required is discovered), and only one pass can sort, dedupe and cap. Emit a stub
 *   expansion ALONGSIDE the errors - without it the missing impl cascades into 'does not
 *   implement' at every use site and buries the real diagnostic"
 *
 * --- GRAMMAR AND RESOLUTION ------------------------------------------------
 *
 * TODO[ ](#shape-attr): C[Attr(shape)],
 *   "The selector. Absent = accept every shape the type implements and let the written Meta
 *   variant choose; present = narrow to the listed ones. Purely additive, so it never restates
 *   what the type already says. Takes several variant paths, making it a MetaList over an enum -
 *   the framework's own grammar dogfooded at the first opportunity. See ID(syntax/no-path-head)
 *   for why the path is an argument and not the head"
 *
 * TODO[ ](#alias-attr): C[Attr(alias)],
 *   "On a field it adds keys; on a type or variant it adds a SEGMENT that joins suffix matching,
 *   so #[alias(Colour)] on ColourSetting makes Colour::Red resolve too. Single idents, since an
 *   alias substitutes for one segment. Justification: with exact matching chosen this is the only
 *   bridging mechanism, so watch for authors writing piles of case aliases - that, and not before,
 *   is the signal a normalisation policy is worth its opinion"
 *
 * TODO[ ](#resolve): C[F(resolve).R(Extraction<Self>)],
 *   "Type-directed: gather the expected type's candidates, match exactly, accept any SUFFIX of a
 *   canonical path, allow a ZST field to be written as key OR value, then zero matches -> 'not
 *   accepted here, expected one of ..' and several -> 'ambiguous, qualify'. Justification: suffix
 *   matching is free for every node and needs nothing declared, and mirroring rustc's own import
 *   semantics means the rule is one users already hold. Keys are idents and values are paths,
 *   exactly the asymmetry Rust has in `Foo { bar: Baz::Qux }` - fields are not items, so there is
 *   no `configuration::colour` to resolve and the qualified key form is dropped"
 *
 * NOTE(#positional): V[S(ConfigName).T(LitStr)],
 *   "Tuple struct = all positional, named struct = all named, no mixing. Justification: Rust has
 *   no named function arguments, so a mixed form has no analogue to borrow intuition from -
 *   forbid it rather than invent a rule nobody can predict. ConfigName stands as the newtype case"
 *
 * TODO[ ](#entry): C[F(from_body).A(\1).T(TokenStream)]
 *   && C[F(from_attributes).A(\1).T(&[Attribute])]
 *   && C[F(from_args).A(\1).T(TokenStream)],
 *   "from_body does the work; the other two are thin adapters. Justification: every attribute-
 *   bearing syn node exposes .attrs, so &[Attribute] is the universal entry and POSITION (item /
 *   field / variant) never needs modelling at all. A proc_macro_attribute hands its args over
 *   already unwrapped, so that path is less work, not different work. Document the one real
 *   asymmetry: empty args have no span, and #[a] is indistinguishable from #[a()] there, so a
 *   bare-flag grammar ROOT works under a derive only"
 *
 * --- THE DERIVE ITSELF -----------------------------------------------------
 *
 * TODO[ ](#derive): C[MacDef(Syntax)],
 *   "Emits the shape impls plus the Node const. Bootstrap: v1 is hand-rolled, because Syntax is
 *   what lets the extractor read its own #[shape(..)] attributes and only then can it be
 *   re-expressed in itself. That self-hosting step is also the first real test of the design"
 *
 * TODO[ ](#testing): C[F(parse_grammar).A(\1).T(&str)],
 *   "Parse a &str into an Attribute and run a grammar against it, so grammar tests need no macro
 *   invocation, plus trybuild snapshots of the messages. Justification: messages are GENERATED
 *   from Reason x Node rather than written by hand, which makes them exactly the output worth
 *   pinning - a regression there is otherwise silent"
 *
 * TODO[ ](#scratch): U[N(scratch)],
 *   "Bring the illustration below in line with the worked example in the header: Vec<ColourSetting>
 *   because arity lives in the type, Other(Ident) because String has no Expr reading and bare Blue
 *   is an ident, Option<NoClean> so every token still resolves to a real item, no configuration::
 *   key prefix, and #[shape(..)] in place of the #[AttributeKind::..] head that
 *   ID(syntax/no-path-head) proved cannot resolve"
 *
 * --- STILL OPEN ------------------------------------------------------------
 *
 * Query(#separator): Q[T(Punctuated<T, Sep>) ??],
 *   "Meta::List::tokens is raw, so Punctuated<T, Token![;]> should let a grammar pick its own
 *   separator - but does rustc accept #[attr(a; b)] as an inert derive helper in the first place?
 *   Unverified. If it does not, the separator knob is decoration and Sep should be dropped from
 *   the forwarding impls in ID(syntax/forwarding)"
 *
 * Query(#custom-reason): Q[E(Reason).V(Custom).T(String) != T(Error)],
 *   "Should the escape hatch carry a String or a fully-formed syn::Error? String keeps the
 *   framework in charge of span and position, which is the property ID(syntax/diagnostics) exists
 *   to protect; syn::Error lets an author report something genuinely structural we have no reason
 *   for. Leaning String - decide before ID(syntax/reason) is written"
 */
