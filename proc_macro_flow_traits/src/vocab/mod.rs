// @review [ ]
//! Declared vocabularies: a closed set of names THIS FRAMEWORK owns, as a real Rust enum with the
//! native conversions.
//!
//! The problem it removes is hand-written spelling comparisons scattered through the crate -
//! `path.is_ident("shape") || path.is_ident("alias")` and the `match ident.to_string().as_str()`
//! that used to sit beside it. Those are unsearchable, drift apart, and put the candidate list for
//! a diagnostic in a different place from the names it lists. A vocabulary declares the set once
//! and generates the comparisons, so call sites read as ordinary Rust:
//!
//! ```ignore
//! let helper: SyntaxHelper = attribute.path().try_into()?;
//! match helper {
//!     SyntaxHelper::Shape => ..,
//!     SyntaxHelper::Alias => ..,
//! }
//! ```
//!
//! NOTE(#vocabulary/only-what-we-own): V[M(vocabulary).!applies(Path(ColourSetting::Red))], "This is
//! for names the FRAMEWORK owns - helper attribute heads, field keys - where the spelling IS the
//! identity because we invented it. It must NOT be pointed at value paths. VERIFIED why: a
//! spelling comparison cannot see through a qualified path or a renamed import, so `MyState::Active`
//! and `use MyState::Active as A;` both fail a match written against `\"Active\"`. Values are rustc's
//! to resolve - they get re-emitted and checked at the splice site, which is where did-you-mean and
//! import suggestions come from for free. Using a vocabulary there would reintroduce exactly the
//! string matching that decision removed, in a more convenient wrapper. See ID(no-type-alias) for
//! the general rule: alias what we own, never what rustc owns"
//!
//! NOTE(#vocabulary/exact): V[M(vocabulary).!folds], "MATCHING is exact - no case folding on the
//! way in. That part is principled: a normalisation rule is a second source of truth about what a
//! name is, and the moment one exists every reader has to know it. Extra spellings are declared as
//! explicit aliases, the same bargain STRICT MATCHING, LENIENT SUGGESTIONS already takes"
//!
//! TODO[~](#vocabulary/derive-spelling):U[M(vocabulary).A(spelling)], "HALF DONE, and the halves
//! split exactly where predicted. ID(syntax/derive) now derives the canonical spelling with
//! heck::ToSnakeCase at EXPANSION time and emits a literal, so a grammar author writes no spelling
//! at all. M(vocabulary) still requires one, and always will: the blocker named below is real and
//! unmovable - macro_rules substitutes token trees, `stringify!($variant)` yields \"Shape\" as a
//! literal, and nothing declarative can lowercase it. The macro stays the hand-written escape
//! hatch; the derive is the normal path. ORIGINAL: "Writing `Shape = \"shape\"`
//! is redundant where the spelling is just the variant in snake_case, and that redundancy is NOT
//! defended by ID(vocabulary/exact) - deriving the ONE canonical spelling at generation time is a
//! convention, not a matching rule, and leaves matching exactly as strict. The reason it is written
//! out here is mechanical: macro_rules! substitutes token trees and has no string manipulation, so
//! it cannot case-convert an ident. `stringify!($variant)` yields \"Shape\" as a literal and nothing
//! in a declarative macro can lowercase it. Doing it at RUNTIME with heck would cost the const fn
//! and an allocation to save some typing, which is a bad trade. This lands when the derive does:
//! a proc macro can call heck::ToSnakeCase at expansion time and emit the literal, making the
//! spelling optional here and explicit only where it differs"
//!
//! Answer(#vocabulary/heck-scope):A[T(heck) == derive_only], "ANSWERED by building it. heck is a
//! dependency of proc_macro_flow_derive and of NOTHING ELSE - the traits crate never sees it,
//! exactly as this query reasoned. It is spent on two translations, both at expansion time: a
//! grammar type's name becomes its entry attribute head (`Configuration` -> `configuration`), and
//! Attr(alias) with no arguments becomes the standard case set. Both emit LITERALS, so matching
//! stays exact per ID(vocabulary/exact) - the conversion is a convention applied once at
//! generation, never a normalisation rule applied at match time. ORIGINAL:
//! - it is the derive's OWN translations, where a grammar type's name becomes its entry attribute
//! head (`Configuration` -> `configuration`, UpperCamel -> snake). That conversion has no home yet
//! because ID(syntax/derive) does not exist. Adding the dependency before something calls it would
//! be premature, so it is named here and not in any Cargo.toml"

//! NOTE(#vocab/match-or-splice): V[M(vocab).matches <=> value.required], "THE rule the whole suite
//! sits on, and the one that resolves an apparent contradiction with ID(no-type-alias). MATCH NAMES
//! ONLY WHERE AN EXPANSION-TIME VALUE IS REQUIRED; SPLICE EVERYWHERE ELSE. rustc can CHECK a name
//! but cannot HAND US a value, so a grammar node the derive must branch on has to be matched by
//! name, while a selector the derive only re-emits should not be - that is where did-you-mean and
//! import suggestions come from for free. The bounded cost of matching, written down rather than
//! discovered: `use ColourSetting::Other as O;` then `colour(O(Blue))` is NOT seen. Suffix matching
//! still handles the qualified form, so `ColourSetting::Red` and `Red` are the same node"
//!
//! NOTE(#vocab/orphan-shapes-the-api): V[Tr(FromExpr).local], "Why leaves get a local trait while
//! everything else gets TryFrom. VERIFIED E0117: `impl TryFrom<&syn::Expr> for syn::LitStr` is a
//! foreign trait on a foreign type and does not compile. The obvious fix - newtype every leaf so
//! TryFrom applies - was considered and REJECTED, because an author's grammar names syn's terminals
//! directly: `Retry { times: LitInt }` would become `times: vocab::Int`, putting our wrapper in
//! their public signature and an unwrap at every use. FromExpr is not a workaround for the orphan
//! rule, it is what lets LitInt stay LitInt. The distinction throughout is WHOSE TYPE IT IS -
//! name_value! generates a newtype the AUTHOR names; leaf! never imposes one on syn"

pub mod flags;
pub mod leaves;
pub mod lists;
pub mod names;
pub mod values;
pub mod variants;
pub mod walk;
