// @review [ ]
// NOTE(#demo/exercises-the-surface): V[N(demo).has(grammar, helpers, children, flags)], "The demo exercises the parts that break, not just a pipeline"
// A single field-name list proved only that a pipeline runs. A macro with a helper GRAMMAR, a rule
// on it, registered spellings and a real child per field exercises the parts that break - and found
// three crate bugs the moment it was written

//! `#[derive(Columns)]` — a worked macro, written the way an AUTHOR would write one.
//!
//! It exists twice over: to compile the entry point Attr(pipeline) generates, and to be the thing a
//! downstream crate actually uses. N(proc_macro_flow_demo_user) is that crate, and between them
//! they are the whole chain - traits, derive, macro author, user.
//!
//! What it does: over a struct of named fields, it writes a `COLUMNS` table. Each field may carry
//! `#[column(..)]` to rename it, mark it a key, or leave it out.
//!
//! ```ignore
//! #[derive(Columns)]
//! struct User {
//!     #[column(rename = "user_id", key)]
//!     id: u64,
//!     email: String,
//!     #[column(skip)]
//!     cache: Vec<u8>,
//! }
//! ```
//!
// NOTE(#demo/exercises-the-whole-surface): V[N(demo).has(grammar, helpers, children, asserts)], "Richer than a field-name list: grammar, rule, helpers, children"
//
// Deliberately richer than one field-name list. The first demo used none of the crate's own
// vocabulary and so proved only that a pipeline runs. This one declares a GRAMMAR for its helper
// attribute, states a RULE that grammar must satisfy, registers the helper through `helpers =`
// so no spelling is written twice, and has a real CHILD EXTRACTION per field - which means the
// diagnostic walk has somewhere to walk and a bad attribute is reported against the field that
// carries it, not the struct.
//!
// NOTE(#demo/why-a-third-crate): V[N(demo).is(proc_macro) && N(demo) != N(derive)], "Two rustc constraints leave this third crate as the only place"
// This crate exists because two rustc constraints meet and leave nowhere else to stand.
//
// (1) `can't use a procedural macro from the same crate that defines it` - VERIFIED, and already
// recorded as ID(derive/cannot-self-host). So proc_macro_flow_derive cannot apply its own
// Attr(pipeline), which is what the plan's 'regenerate lib.rs::field_names with Attr(pipeline)'
// asked for and why that step is impossible rather than merely unfinished.
//
// (2) `functions tagged with #[proc_macro_derive] must currently reside in the root of the crate`,
// and only a `proc-macro = true` crate may have one at all. So the facade cannot host it either -
// which is the one thing ID(facade/hosts-the-proof) could never cover.
//
// The gap that leaves is the one this closes. Every proof of Attr(pipeline) so far ran under
// `entry = manual`, so the entry function it generates had been asserted as TOKENS and never once
// handed to rustc. Here it is compiled, exported, and used by the facade against a real struct.

use proc_macro_flow_derive::pipeline;

// MUST BE AT THE CRATE ROOT. Attr(pipeline) emits the entry function as a SIBLING of this module,
// and (2) above is why that is the only place it can land - ID(pipeline/entry-is-a-sibling).
#[pipeline(derive = Columns)]
mod columns {
    use proc_macro_flow_derive::{Diagnose, Extractor, Generator, Processor, Syntax};
    use proc_macro_flow_traits::{
        extractor::{Extracted, Extraction, Extractor, Reason, ReasonKind, Validate},
        flag,
        processor::Processor,
        quote::quote,
        syn::{self, parse2, Data, DeriveInput, Field, FieldsNamed, Ident, ItemImpl, LitStr},
        vocab::leaves::FromMeta,
        vocabulary,
    };

    vocabulary! {
        /// The helper attributes this derive registers, and the only heads it owns.
        ///
        /// Named ONCE. `helpers = ColumnHelper` on the extractor role lifts these spellings into
        /// the generated `attributes(..)`, so a rename here is a rename everywhere.
        pub enum ColumnHelper {
            Column = "column",
        }
    }

    flag! {
        /// `key` — written bare, and presence is the whole signal.
        pub struct Key = "key";
    }

    flag! {
        /// `skip` — likewise.
        pub struct Skip = "skip";
    }

    /// What `#[column(..)]` accepts.
    ///
    /// An ordinary grammar: arity comes off each field's type, `#[alias]` adds the case spellings,
    /// and the rule is checked wherever the value is walked.
    #[derive(Syntax)]
    #[assert(conflicts(skip, key))]
    pub struct Column {
        #[alias]
        pub rename: Option<LitStr>,
        pub key: Option<Key>,
        pub skip: Option<Skip>,
    }

    /// One field of the annotated struct, and what `#[column(..)]` said about it.
    ///
    /// HAND-WRITTEN, and the only stage here that should be: reading the grammar can FAIL, and a
    /// failure belongs on the field that carries the attribute. A derived extractor splices
    /// expressions and has nowhere to put a E(Reason) - so the moment reading is fallible, the
    /// stage is doing real work and writes itself. Attr(derive(Diagnose)) still absorbs the walk.
    ///
    /// `column` is WALKED, not skipped. The walk finds nothing beneath a grammar, but the derived
    /// Assert asks it on the way past, and that is the only route by which `conflicts(skip, key)`
    /// reaches the author - ID(diagnose-derive/asks-what-it-walks).
    #[derive(Diagnose)]
    pub struct ColumnRead<'ast> {
        #[skip]
        pub name: Option<&'ast Ident>,
        pub column: Option<Column>,
    }

    impl<'ast> Validate<'ast> for ColumnRead<'ast> {
        type Source = &'ast Field;
        type Valid = &'ast Field;

        fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
            Ok(input)
        }
    }

    impl<'ast> Extractor<'ast> for ColumnRead<'ast> {
        type Output = Extracted<Self, &'ast Field>;

        fn extract_from(node: &'ast Field) -> Self::Output {
            let mut out: Extraction<Self> = Extraction::default();

            let mut written = node.attrs.iter().filter(|a| a.path().is_ident("column"));
            let first = written.next();

            // A second `#[column]` is neither merged nor dropped. The first is still read, and
            // every extra one is a complaint pointing at itself - a value AND a reason, which is
            // the case S(Extraction) is shaped for.
            for extra in written {
                out.reasons.push(Reason::at(ReasonKind::Duplicate, extra));
            }

            let column = match first.map(|attr| Column::from_meta(&attr.meta)) {
                None => None,
                Some(Ok(column)) => Some(column),
                // The grammar already worded this - a missing key, an unknown one with its
                // did-you-mean. It is carried, not rephrased, and the S(Extracted) below spans it
                // against THIS field rather than the whole struct.
                Some(Err(error)) => {
                    out.reasons.push(Reason::new(ReasonKind::Syntax(error)));
                    None
                }
            };

            out.value = Some(ColumnRead {
                name: node.ident.as_ref(),
                column,
            });

            Extracted::new(out, node)
        }
    }

    impl<'ast> Processor<'ast> for ColumnRead<'ast> {
        type Input = Extracted<ColumnRead<'ast>, &'ast Field>;
        type Output = Self;

        fn process(input: Self::Input) -> Extraction<Self::Output> {
            input.into_extraction()
        }
    }

    /// The whole struct: its name, and one child extraction per field.
    ///
    /// `cols` IS a child, so the walk descends into it and a complaint about one field is spanned
    /// against THAT field.
    #[derive(Extractor, Processor)]
    #[source(DeriveInput)]
    #[extractor(source = ::proc_macro_flow_traits::syn::DeriveInput, helpers = ColumnHelper)]
    #[processor(from = TableRead)]
    pub struct TableRead<'ast> {
        #[value(source.0)]
        pub item: &'ast Ident,
        #[from(source.1.named.iter())]
        pub cols: Vec<Extracted<ColumnRead<'ast>, &'ast Field>>,
    }

    /// The only hand-written stage, and the only one that should be: a narrowing is a decision.
    ///
    /// NOTE(#demo/valid-is-what-the-stage-needs): V[Ty(Valid).is(pair)], "Valid narrows to what this stage needs: the name and the fields"
    /// Attr(derive(Validate)) declines to guess at a narrowing, so this is written out. It narrows
    /// to a PAIR rather than the `&FieldsNamed` the shape check produces, because every Attr(value)
    /// below is written against `source`: narrowing to the fields alone would put the type's own
    /// name out of reach. Ty(Valid) is exactly the place to say what this stage needs.
    impl<'ast> Validate<'ast> for TableRead<'ast> {
        type Source = &'ast DeriveInput;
        type Valid = (&'ast Ident, &'ast FieldsNamed);

        fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
            let Data::Struct(data) = &input.data else {
                return Err(Reason::at(ReasonKind::WrongShape, &input.ident));
            };

            match &data.fields {
                syn::Fields::Named(named) => Ok((&input.ident, named)),
                _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
            }
        }
    }

    /// One impl, and the type says one - ID(generation/newtype-per-item).
    #[derive(Generator)]
    #[builds(from = TableRead<'ast>, subject = &'ast DeriveInput)]
    pub struct Table(ItemImpl);

    /// The generator ROLE, on an alias: the wiring names every stage `Name<'ast>`, and a generator
    /// wrapping a syn item borrows nothing.
    #[generator(from = TableRead)]
    pub type Built<'ast> = Table;

    impl Table {
        fn assemble(input: &TableRead<'_>) -> syn::Result<Self> {
            let item = input.item;
            let mut columns = Vec::new();

            for col in &input.cols {
                let Some(read) = col.value() else { continue };
                let Some(name) = read.name else { continue };

                // Already read, and already complained about if it was malformed - see
                // S(ColumnRead). Generation gets a value or nothing, and never a parse.
                let declared = read.column.as_ref();

                if declared.is_some_and(|c| c.skip.is_some()) {
                    continue;
                }

                let spelled = declared
                    .and_then(|c| c.rename.as_ref().map(|lit| lit.value()))
                    .unwrap_or_else(|| name.to_string());
                let is_key = declared.is_some_and(|c| c.key.is_some());

                columns.push(quote!((#spelled, #is_key)));
            }

            parse2(quote! {
                impl #item {
                    /// Every column: its name, and whether it is a key.
                    pub const COLUMNS: &'static [(&'static str, bool)] = &[ #(#columns),* ];
                }
            })
            .map(Table)
        }

        /// The vacant form is the same SHAPE, so a failure does not cascade into "no associated
        /// item" at every use site - ID(generator/stub-is-not-empty).
        fn assemble_stub(subject: &DeriveInput) -> syn::Result<Self> {
            let item = &subject.ident;
            parse2(quote! {
                impl #item {
                    pub const COLUMNS: &'static [(&'static str, bool)] = &[];
                }
            })
            .map(Table)
        }
    }
}

// The STAGE, run in process - what a grammar-level test cannot see. `Column`'s own Assert was
// always correct; what broke was the stage never asking it. So these read a real field, walk the
// extraction exactly as the pipeline does, and look at what comes out.
#[cfg(test)]
mod tests {
    use super::columns::ColumnRead;
    use proc_macro_flow_traits::{
        extractor::Extractor,
        render::Diagnose,
        syn::{self, parse_str, Data, DeriveInput, Fields},
    };

    /// Every complaint the walk finds on the one field of `struct T { <field> }`.
    fn complaints(field: &str) -> Vec<String> {
        let input: DeriveInput = parse_str(&format!("struct T {{ {field} }}")).expect("parses");
        let Data::Struct(data) = &input.data else { unreachable!() };
        let Fields::Named(named) = &data.fields else { unreachable!() };
        let field: &syn::Field = named.named.first().expect("one field");

        ColumnRead::extract_from(field)
            .render()
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn a_rule_the_grammar_states_is_enforced_by_the_stage() {
        // ID(diagnose-derive/asks-what-it-walks). This compiled clean before - the rule was
        // stated on the grammar and never once asked.
        let found = complaints("#[column(skip, key)] id: u64");

        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("cannot be written together"), "{found:?}");
    }

    #[test]
    fn a_second_column_attribute_is_a_complaint_not_a_silent_drop() {
        let found = complaints(r#"#[column(key)] #[column(rename = "x")] id: u64"#);

        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("more than once"), "{found:?}");
    }

    #[test]
    fn a_clean_field_says_nothing() {
        // The other side of both: walking and asking a grammar that breaks nothing adds nothing.
        assert!(complaints(r#"#[column(rename = "x", key)] id: u64"#).is_empty());
        assert!(complaints("id: u64").is_empty());
    }
}
