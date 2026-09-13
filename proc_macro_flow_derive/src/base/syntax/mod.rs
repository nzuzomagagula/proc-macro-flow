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
 * Only what this crate actually builds. The public trait surface - the three shape traits,
 * FromExpr, Reason, Extraction, Node, Diagnostics and the parse entry points - is tracked in
 * proc_macro_flow_traits, because a proc-macro crate cannot export any of it. See
 * ID(syntax/placement) there; this crate only ever IMPLEMENTS and EMITS those items.
 *
 * NOTE(#no-path-head): V[Attr(shape) != Attr(AttributeKind::MetaList)], "The selector must stay a SINGLE-SEGMENT helper attribute taking the variant path as an ARGUMENT. VERIFIED: #[AttributeKind::MetaList] fails with `cannot find type AttributeKind in this scope` - attribute heads resolve in the macro namespace, enum variants are not in it, and derive helpers are registered as bare idents with no path to them. Unfixable, not merely inconvenient. Inside the delimiters rustc resolves nothing, so the path survives as tokens and can be re-emitted into generated code, where it does resolve"
 *
 * NOTE(#positional): V[S(ConfigName).T(LitStr)], "Tuple struct = all positional, named struct = all named, no mixing. Rust has no named function arguments, so a mixed form has no analogue to borrow intuition from - forbid it rather than invent a rule nobody can predict. ConfigName stands as the newtype case"
 *
 * TODO[ ](#derive): C[MacDef(Syntax)], "The derive itself: emit the shape impls plus the Node const. Bootstrap - v1 is hand-rolled, because Syntax is what lets the extractor read its own #[shape(..)] attributes and only then can it be re-expressed in itself. That self-hosting step is also the first real test of the design. Blocked on ID(syntax/traits) and ID(syntax/node-table) existing to implement against"
 *
 * TODO[ ](#shape-attr): C[Attr(shape)], "The selector. Absent = accept every shape the type implements and let the written Meta variant choose; present = narrow to the listed ones. Purely additive, so it never restates what the type already says. Takes several variant paths, making it a MetaList over an enum - this framework's own grammar dogfooded at the first opportunity. See ID(syntax/no-path-head) for why the path is an argument and not the head. Must also be registered in the derive's attributes(..) list, which nothing auto-syncs - a missed name fails at the USER's site"
 *
 * TODO[ ](#alias-attr): C[Attr(alias)], "On a field it adds keys; on a type or variant it adds a SEGMENT that joins suffix matching, so #[alias(Colour)] on ColourSetting makes Colour::Red resolve too. Single idents, since an alias substitutes for one segment. With exact matching chosen this is the only bridging mechanism, so watch for authors writing piles of case aliases - that, and not before, is the signal a normalisation policy is worth its opinion"
 *
 * TODO[ ](#scratch): V[N(scratch).has(S(Configuration))] && V[N(scratch).has(E(ColourSetting))], "The maximal grammar at the foot of this file - every shape, arity rule and resolution rule in one pair of items, and the thing to check any behaviour change against. It is GATED behind #[cfg(any())] and does not compile, deliberately: Syntax, SomeDerive and the proc_macro_flow_traits::syntax support types are all still unwritten, so the errors it raises are a live checklist of what ID(syntax/traits), ID(syntax/forwarding) and ID(syntax/derive) still owe it. The attribute BODIES are verified to parse as Meta spine plus Expr leaves, so any parse failure here is a regression and not a missing feature. Mapping table and rejection cases in SCRATCH.md. NOTE that a second, SMALLER worked example now lives beside it in worked.rs, whose layer 3 does compile and is asserted - scratch remains the maximal grammar to check behaviour against, worked.rs is the minimal one that actually runs"
 *
 * --- STILL OPEN ------------------------------------------------------------
 *
 * Query(#separator): Q[T(Punctuated<T, Sep>) ??], "Meta::List::tokens is raw, so Punctuated<T, Token![;]> should let a grammar pick its own separator - but does rustc accept #[attr(a; b)] as an inert derive helper in the first place? Unverified. If it does not, the separator knob is decoration and Sep should be dropped from the forwarding impls in ID(syntax/forwarding)"
 *
 * Query(#custom-reason): Q[E(Reason).V(Custom).T(String) != T(Error)], "Should the escape hatch carry a String or a fully-formed syn::Error? String keeps the framework in charge of span and position, which is the property ID(syntax/diagnostics) exists to protect; syn::Error lets an author report something genuinely structural we have no reason for. Leaning String - decide before ID(syntax/reason) is written"
 */

pub mod extractor;
pub mod worked;

// The maximal grammar: every shape, every arity rule, every resolution rule the stage has to
// handle, in one pair of items. Does NOT compile - Syntax/SomeDerive and the support types do not
// exist yet - but the attribute bodies below are VERIFIED to parse: both spines as
// Punctuated<Meta, Comma>, every nested body as Meta, every leaf as Expr. Mapping table and the
// rejection cases: see SCRATCH.md beside this file.
// GATED(#syntax/scratch-gate): #[cfg(any())] is "never compile". The scratch is a reference
// document that happens to be written in Rust, and it names Syntax/SomeDerive and the
// proc_macro_flow_traits::syntax support types that do not exist yet - so while it was a live
// `mod` no `cargo check` in this crate could ever be green, which costs far more than the
// 18 errors it raises are worth. Ungate it to re-run the checklist once ID(syntax/traits) and
// ID(syntax/derive) land; it is meant to compile eventually, and that is the signal they are done.
#[cfg(any())]
pub mod scratch {
    use proc_macro_flow_traits::syntax::{AttributeKind, NonEmpty, Spanned};
    use syn::{punctuated::Punctuated, Expr, Ident, LitInt, LitStr, Path, Token};

    #[derive(Syntax)]
    #[alias(config)]                        // TYPE alias -> #[config(..)] is the same entry
    pub struct Configuration {              // entry name `configuration`, from the type name
        #[shape(AttributeKind::MetaList)]   // narrowed: `colour = Red` is rejected here
        colour: Vec<ColourSetting>,         // required (not Option) + many (Vec)

        fallback: Option<ColourSetting>,    // no #[shape] -> any shape ColourSetting implements

        retry: Option<Retry>,               // named-field struct  -> nested list
        bounds: Option<Bounds>,             // tuple struct        -> positional
        #[alias(label)]
        name: Option<ConfigName>,           // newtype over a leaf -> name-value
        no_clean: Option<NoClean>,          // ZST                 -> flag

        #[shape(AttributeKind::NamedValue)] // bool's other reading; FromPath makes it a flag
        verbose: Option<bool>,

        #[shape(AttributeKind::MetaList)]
        targets: Option<NonEmpty<Spanned<Path>>>, // >=1, and keep each token's span

        #[shape(AttributeKind::MetaList)]
        sizes: Option<Punctuated<LitInt, Token![,]>>, // separator carried by the type

        guard: Option<Expr>,                // free-form leaf: any Rust expression
        nested: Option<Box<Configuration>>, // Box -> recursive grammar
    }

    #[derive(Syntax)]
    #[alias(Colour)] // adds a SEGMENT: Colour::Red resolves as well as ColourSetting::Red
    pub enum ColourSetting {
        Red, // unit variant -> Path
        #[alias(noir)]
        Black,
        Other(Ident),                            // tuple variant  -> List, payload read as Expr
        Rgb { r: LitInt, g: LitInt, b: LitInt }, // struct variant -> List of name-values
    }

    #[derive(Syntax)]
    pub struct Retry {
        times: LitInt,
        backoff: Option<LitStr>,
    }

    #[derive(Syntax)]
    pub struct Bounds(LitInt, LitInt);

    #[derive(Syntax)]
    pub struct ConfigName(LitStr);

    #[derive(Syntax)]
    pub struct NoClean;

    // Downstream. Two attributes fold into one Configuration: a scalar written twice across them
    // is a Duplicate, `colour` would accumulate.
    #[derive(SomeDerive)]
    #[configuration(
        colour(ColourSetting::Red, Other(Blue), Rgb(r = 12, g = 34, b = 56)),
        fallback = Colour::Black,
        retry(times = 3, backoff = "200ms"),
        bounds(0, 64),
        name = "thing",
        NoClean,
    )]
    #[config(
        verbose = true,
        targets(::core::fmt::Debug, my_crate::Thing),
        sizes(1, 2, 4),
        guard = cfg!(debug_assertions),
        nested(colour(Black)),
    )]
    pub struct Thing;
}
