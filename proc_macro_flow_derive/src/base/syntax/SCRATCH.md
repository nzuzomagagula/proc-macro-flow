# The syntax stage — reference

Attribute grammars declared as ordinary Rust items. The canonical form of every node is the real
path to the item that declares it, so parsing is **resolution against those items**, not matching
token shapes. Aliases and path shortening are the ergonomics layer on top; the long form is always
writable.

The grammar itself lives in `mod.rs::scratch`. This file is the map for reading it.

---

## The one structural rule

> **Meta goes as deep as there are named nodes. Expr takes over the moment a value is expected.**

`Rgb(r = 12)` stays `Meta` because `r` names something. `bounds(0, 64)` drops to `Expr` because `0`
names nothing — and bare literals are not valid `Meta` at all, which is why `Expr` has to exist as
the leaf grammar rather than being a convenience.

Two things make this cheap: `syn::MetaNameValue::value` is *already* an `Expr`, so the right-hand
side of `=` needs no parser of ours; and `Meta::List::tokens` is a raw `TokenStream`, so a terminal
node simply chooses `Punctuated<Expr, Comma>` instead of `Punctuated<Meta, Comma>`.

---

## What each written form maps to

| Written | Meta variant | Trait called | Resolves to | Note |
|---|---|---|---|---|
| `colour(..)` | List | `FromMetaList` | `Vec<ColourSetting>` | arity from `Vec`, not from the shape |
| `ColourSetting::Red` | Path | `FromPath` | unit variant | canonical, fully qualified |
| `Other(Blue)` | List | `FromMetaList` | tuple variant | **suffix match**: `Other` ≡ `ColourSetting::Other` |
| `Blue` | — | `FromExpr` | `Ident` | payload; Meta stops, Expr starts |
| `Rgb(r = 12, ..)` | List | `FromMetaList` | struct variant | spine recurses: elements are Meta again |
| `fallback = Colour::Black` | NameValue | `FromNameValue` | variant | `Colour::` is the **type alias** segment |
| `retry(times = 3, ..)` | List | `FromMetaList` | nested struct | `times` required, `backoff` optional |
| `bounds(0, 64)` | List | `FromExpr` ×2 | tuple struct | bare literals — **impossible without Expr** |
| `name = "thing"` | NameValue | `FromNameValue` | newtype | `label = "thing"` equivalent (field alias) |
| `NoClean` | Path | `FromPath` | ZST | **key-or-value**: `no_clean` equivalent |
| `verbose = true` | NameValue | `FromNameValue` | `bool` | `#[shape]` picks this over the flag reading |
| `targets(::core::..)` | List | `FromPath` ×2 | `Spanned<Path>` | `NonEmpty` rejects `targets()` |
| `sizes(1, 2, 4)` | List | `FromExpr` ×3 | `LitInt` | separator from `Punctuated<_, Token![,]>` |
| `guard = cfg!(..)` | NameValue | `FromExpr` | `Expr` | arbitrary expression passes through |
| `nested(colour(..))` | List | `FromMetaList` | `Box<Configuration>` | recursion terminates on finite input |

---

## What it must reject, and where the reason attaches

| Written | Reason | Attaches to |
|---|---|---|
| `colour = Red` | `WrongShape` | the field — `#[shape]` excluded NameValue |
| `Other(Blue, Teal)` | `WrongShape` | the variant — arity 1 from `Other(Ident)` |
| `targets()` | `Missing` | the field — `NonEmpty` |
| `colur(Red)` | `UnknownKey` | **the parent** — no node owns it |
| `name = "a", label = "b"` | `Duplicate` | **the parent** — scalar written twice |
| `bounds(0, 64, 9)` | `WrongShape` | the field — `Bounds` is a 2-tuple |
| `Rgb(12, 34, 56)` | `WrongShape` | the variant — named struct, no mixing |

The two parent-attached rows are load-bearing. They are why `Extraction<T>` is
`{ value: Option<T>, reasons: Vec<Spanned<Reason>> }` and not a three-state enum: a parent can
extract perfectly well *and* carry a complaint of its own, which no `Ok | Failed | Absent` can say.

---

## Decisions, and what forced each

| | |
|---|---|
| **Shape is selected, not validated** | A type may implement several shapes; the parent narrows. `#[shape]` absent = accept all the type implements. It lowers to a trait **bound**, so a bad selection fails in the *author's* crate at declaration time. |
| **Arity and requiredness live in the type** | `T` required, `Option<T>` optional, `Vec<T>`/`NonEmpty<T>` repeated, `Punctuated<T, Sep>` carries its separator. No `#[required]`, no `#[default]` — a second source of truth that can disagree with the type is the failure mode being avoided. |
| **Exact matching, explicit aliases** | No case normalisation. `NoClean` works for field `no_clean` not by case-bridging but because a ZST field accepts its key *or* its value. |
| **Strict matching, lenient suggestions** | Resolution is case-sensitive; the did-you-mean search is not. Leniency belongs where a wrong guess is free. |
| **Suffix matching is not aliasing** | A written path matches a node when it is a suffix of that node's canonical path — free for every node, with rustc's own ambiguity semantics. |
| **Keys are idents, values are paths** | Exactly Rust's own asymmetry in `Foo { bar: Baz::Qux }`. Fields are not items, so `configuration::colour` resolves to nothing and the qualified key form is dropped. |
| **Closed reasons, open rendering** | `Reason` is a fixed set so the framework can render it against the `Node` table; `Diagnostics` lets an author rephrase but never construct — the span and tree position stay with the framework. |
| **`extract_from` returns `Extraction<Self>`, never `Result`** | No `?`, no early return, no path that discards a node — losing a sibling becomes unrepresentable rather than discouraged. |

---

## Verified against rustc / syn

Four constraints were checked rather than assumed. Re-check before contradicting any of them.

| Claim | Result |
|---|---|
| `#[AttributeKind::MetaList]` as an attribute **head** | **Fails** — `cannot find type AttributeKind in this scope`. Heads resolve in the macro namespace, where variants do not exist, and derive helpers are bare idents with no path to them. Inside the delimiters rustc resolves nothing, so the path survives as tokens. |
| `configuration.colour(..)` as `Meta` | **Fails** — `expected ','`. A dotted receiver is not Meta. `configuration::colour(..)` parses fine, multi-segment path intact. |
| `pub` non-macro item in a proc-macro crate | **Rejected outright** — *"proc-macro crate types cannot export any items other than functions tagged with `#[proc_macro*]`"*. A **private** one is legal and parses fine, so expansion-time resolution works either way; `pub` in a lib crate is what reaches cargo doc and lets a re-emitted path resolve downstream. |
| `syn::Error::combine` + `to_compile_error` | Keeps each error's own span and emits **one `compile_error!` per error** — all-at-once reporting works on stable, no nightly diagnostics needed. |

The scratch's attribute bodies are verified to parse: both spines as `Punctuated<Meta, Comma>`,
every nested body as `Meta`, every leaf as `Expr`. A parse failure there is a regression, not a
missing feature.

---

## Still open

- **`#syntax/separator`** — `Meta::List::tokens` is raw, so `Punctuated<T, Token![;]>` *should*
  work, but it is unverified whether rustc accepts `#[attr(a; b)]` as an inert derive helper at
  all. If it does not, `Sep` is decoration and should come out of the forwarding impls.
- **`#syntax/custom-reason`** — should the escape hatch carry a `String` or a full `syn::Error`?
  `String` keeps the framework in charge of span and position; `syn::Error` lets an author report
  something genuinely structural. Leaning `String`.

## Where the work is tracked

| | |
|---|---|
| `proc_macro_flow_traits/src/lib.rs` | the public surface — shape traits, `FromExpr`, `Reason`, `Node`, `Diagnostics`, entry points |
| `proc_macro_flow_traits/src/extractor.rs` | `Extraction<T>` and the `Result` → `Extraction` change |
| `base/syntax/mod.rs` | this crate's half — the derive, `#[shape]`, `#[alias]`, the scratch |

`nuts list` for the tasks; goals are per-crate in each `.nuts/goals.toml`.
