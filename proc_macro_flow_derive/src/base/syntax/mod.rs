// @review [ ]
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
 * NOTE(#placement): V[N(proc_macro_flow_traits)], "Shape traits, Reason,
 *   Extraction and Node live in the ordinary lib crate, never here. VERIFIED
 *   that a pub non-macro item in a proc-macro crate is rejected by rustc, so
 *   this is a language constraint and not a preference. Blocks every other
 *   #syntax task; shares its fix with #pipeline/relocate-traits"
 *
 * NOTE(#no-path-head): V[Attr(shape)], "The shape selector must stay a
 *   SINGLE-SEGMENT helper attribute whose argument is the variant path.
 *   VERIFIED that #[AttributeKind::MetaList] fails to resolve: attribute heads
 *   resolve in the macro namespace, enum variants are not in it, and derive
 *   helpers are bare idents with no path to them. Do not retry the qualified
 *   head form - it is unfixable, not merely inconvenient"
 *
 * TODO[ ](#traits): C[Tr(FromPath)] && C[Tr(FromMetaList)] && C[Tr(FromNameValue)],
 *   "One trait per attribute shape, so #[shape(..)] lowers to a trait bound
 *   instead of a runtime match on AttributeKind. Justification: a type that was
 *   never declared parsable in the selected shape must fail in the AUTHOR's
 *   crate at declaration time, which trait resolution gives for free and a
 *   runtime enum cannot. Three and only three, because syn::Meta has exactly
 *   three variants - that is Rust's real attribute grammar, not our taxonomy"
 *
 * TODO[ ](#leaves): C[Tr(FromExpr)], "Leaf trait for value positions, plus impls
 *   for the syn terminals (Ident, Path, Type, the Lit* family, Expr) and the
 *   primitives bridged from literals (String <- LitStr, integers <- LitInt,
 *   bool <- LitBool). Justification: Meta cannot represent bare literals, so
 *   `sizes(1, 2)` needs Expr underneath; Meta::List::tokens being raw is what
 *   lets a terminal node choose this parser instead"
 *
 * TODO[ ](#bool-double-duty): V[Tr(FromPath).impl(bool)], "bool implements BOTH
 *   FromPath (a flag) and FromNameValue (a literal). Recorded deliberately so
 *   nobody later 'fixes' it: shape selection resolves the ambiguity, which is
 *   the whole point of shapes being a capability set rather than a property"
 *
 * TODO[ ](#forwarding): C[Impl(FromMetaList)], "Adapters for Option<T>, Vec<T>,
 *   NonEmpty<T>, Punctuated<T, Sep>, Box<T> and Spanned<T>. Justification: this
 *   is where requiredness and arity are enforced, and putting them here keeps
 *   'how many' in exactly one place - the field type - rather than smeared
 *   across the shape traits. Box<T> is what makes recursive grammars work;
 *   Spanned<T> is the opt-in span boundary so plain types stay plain"
 *
 * TODO[ ](#node-table): C[S(Node)], "Reflection const emitted by the derive:
 *   name, aliases, accepted shapes, children. Justification: this single table
 *   is what pays for 'expected one of ..', 'did you mean ..', and 'colour is a
 *   list here, not a name-value'. Generating meaning from a table beats an error
 *   taxonomy, and it is the honest answer to #extractor/error. Later it also
 *   gives generated grammar documentation for near-free"
 *
 * TODO[ ](#reason): C[E(Reason)], "Closed reason set - WrongShape, UnknownKey,
 *   Missing, Ambiguous, NotResolvable, Custom(String). Justification: CLOSED
 *   REASONS, OPEN RENDERING. Closed so the framework can interpret what it
 *   caught and render it against Node; Custom as the escape hatch so an exotic
 *   grammar is not blocked. Authors never construct a message, so they cannot
 *   produce an unspanned or context-free one"
 *
 * TODO[ ](#extraction): R[E(ExtractionState) -> S(Extraction)], "Replace the
 *   Initialised/Uninitialised enum with { value: Option<T>, reasons:
 *   Vec<Spanned<Reason>> }. Justification: the current Result<ExtractionState<T>,
 *   E> encodes 'did it work' twice, and neither a 2-state enum nor a 3-state one
 *   can say 'this node extracted fine AND has a complaint of its own' - which is
 *   exactly unknown-key, a failure of the PARENT to consume its input. All four
 *   combinations are meaningful: Some/[] clean, Some/[..] partial, None/[..]
 *   failed, None/[] absent. Supersedes #cleanup's typestate framing"
 *
 * TODO[ ](#no-result): U[Tr(Extractor).F(extract_from).R(Extraction<Self>)],
 *   "extract_from must return Extraction<Self>, never Result. Justification:
 *   this is the enforcement. With no Result there is no `?`, no early return,
 *   and no control-flow path that throws a node away, so losing a sibling or a
 *   position becomes unrepresentable rather than merely discouraged. A failed
 *   node is still a node, which is also what lets later stages see WHICH subtree
 *   broke instead of finding a hole"
 *
 * TODO[ ](#diagnostics): C[Tr(Diagnostics)], "Author-overridable RENDERING, with
 *   a blanket default. Justification: scope it to rephrasing, never to
 *   construction - the framework keeps the span and the tree position, so the
 *   worst an author can do is bad prose in the right place. The case that earns
 *   it is domain vocabulary: a DSL wants 'unknown column option', which the
 *   framework cannot know and which should not cost the author spans or
 *   did-you-mean to obtain"
 *
 * TODO[ ](#render): C[F(render)], "One walk over the finished Extraction tree
 *   producing N spanned compile_error!s, sorted by span. Justification: traversal
 *   order is not source order (written keys are visited before missing-required
 *   is discovered), and only a single final pass can sort, dedupe and cap. Emit
 *   a stub expansion ALONGSIDE the errors - without it the missing impl produces
 *   a cascade of 'does not implement' errors at every use site that buries the
 *   real diagnostic"
 *
 * TODO[ ](#entry): C[F(from_body)] && C[F(from_attributes)] && C[F(from_args)],
 *   "from_body does the work; the other two are thin adapters. Justification:
 *   every attribute-bearing syn node exposes .attrs, so &[Attribute] is the
 *   universal entry and POSITION (item / field / variant) never needs modelling.
 *   A proc_macro_attribute hands over its args already unwrapped, so that path
 *   is less work, not different work. Document the one real asymmetry: empty
 *   args have no span, and #[a] is indistinguishable from #[a()] there, so a
 *   bare-flag grammar root works under a derive only"
 *
 * TODO[ ](#shape-attr): C[Attr(shape)], "The selector attribute. Absent = accept
 *   every shape the type implements; present = narrow. Takes several variant
 *   paths, so it is itself a MetaList over an enum - the framework's own grammar
 *   dogfooded at the first opportunity. See #no-path-head for why the variant
 *   path is an argument and not the head"
 *
 * TODO[ ](#alias-attr): C[Attr(alias)], "Explicit aliases: on a field they add
 *   keys, on a type or variant they add a SEGMENT that joins suffix matching, so
 *   #[alias(Colour)] on ColourSetting makes Colour::Red resolve too. Single
 *   idents, since an alias substitutes for one segment. Justification: with
 *   exact matching chosen, this is the ONLY bridging mechanism, so watch for
 *   authors writing piles of case aliases - that, and not before, is the signal
 *   that a normalisation policy is worth its opinion"
 *
 * TODO[ ](#resolve): C[F(resolve)], "Type-directed resolution: gather the
 *   expected type's candidates, match exactly, accept any SUFFIX of a canonical
 *   path, allow a ZST field to be written as key or value, then zero matches ->
 *   'not accepted here, expected one of ..' and several -> 'ambiguous, qualify'.
 *   Justification: suffix matching is free for every node and needs nothing
 *   declared, and mirroring rustc's import semantics means the rule is one
 *   users already hold"
 *
 * TODO[ ](#positional): V[S(ConfigName)], "Tuple struct = all positional, named
 *   struct = all named, no mixing. Justification: Rust has no named function
 *   arguments, so a mixed form has no analogue to borrow intuition from -
 *   forbid it rather than invent a rule nobody can predict"
 *
 * TODO[ ](#derive): C[MacDef(Syntax)], "The derive itself: emit the shape impls
 *   plus the Node const. Bootstrap note - v1 is hand-rolled because Syntax is
 *   what lets the extractor read its own #[shape(..)] attributes, and only then
 *   can it be re-expressed in itself. That self-hosting step is also the first
 *   real test of the design"
 *
 * TODO[ ](#testing): C[F(parse_grammar)], "A helper that parses a &str into an
 *   Attribute and runs a grammar against it, so grammar tests need no macro
 *   invocation at all, plus trybuild snapshots of the messages. Justification:
 *   messages are now GENERATED from Reason x Node rather than written by hand,
 *   which makes them exactly the kind of output worth pinning - a regression
 *   there is silent otherwise"
 *
 * TODO[ ](#scratch): U[B(scratch)], "Bring the illustration below in line with
 *   the worked example in this header: Vec<ColourSetting>, Other(Ident),
 *   Option<NoClean>, no configuration:: key prefix, and #[shape(..)] instead of
 *   the #[AttributeKind::..] head that #no-path-head proved cannot resolve"
 */

pub mod syntax_extractor {
    use syn::Type;
    //FIXME[~](#syntax/implementation):I[this], "The syntax stage is designed but unbuilt - the
    //   task list is in the header block above, and #syntax/placement gates all of it. This FIXME
    //   is the CI gate for the stage as a whole; clear it when the header tasks are done"
    pub struct SyntaxExtractor {
        field: SyntaxFieldExtractor,
    }

    pub struct SyntaxFieldExtractor {
        attribute_kind: AttributeKind,
        required: bool,
        syntax_type: Type,
    }

    pub enum AttributeKind {
        MetaList,
        Path,
        NamedValue,
    }

    mod scratch {
        #[derive(Syntax)]
        pub struct Configuration {
            //TODO[x](#syntax/requiredness):D[Attr(required)], "ANSWERED - no. Requiredness is the
            //   field TYPE: T required, Option<T> optional, Vec<T>/NonEmpty<T> repeated. An
            //   attribute that can disagree with the type is a second source of truth, which is
            //   the darling failure mode this whole stage exists to avoid. Delete the marker"
            #[AttributeKind::MetaList]
            #[required]
            colour: ColourSetting,
            #[AttributeKind::NamedValue]
            name: ConfigName,
            #[AttributeKind::Path]
            no_clean: bool,
        }

        #[derive(Syntax)]
        pub enum ColourSetting {
            Red,
            Black,
            Other(String),
        }

        #[derive(SomeDerive)]
        #[configuration(configuration.colour(ColourSetting::Red, ColorSetting::Other(Blue)), name = "Fuck", NoClean)]
        pub struct Thing;
    }
}

pub mod metalist {
    pub struct MetalistExtractor {
        name: MetalistName,
    }
    pub struct MetaListName(Path);

    pub mod scratch {
        use syn::Token;

        #[derive(MetaList, Syntax)]
        pub struct Operation {
            tables: Punctuated<OperationOption, Token![,]>,
        }

        #[derive()]
    }
}
