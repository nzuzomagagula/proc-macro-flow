// @review [ ]
//! Reading an extraction type: its `#[source]`, and where each field's children come from.
//!
//! NOTE(#extractor-derive/declaration-is-a-child): V[S(ExtractorExtraction).P(declaration)], "The
//! `#[source(Ty)]` reading is a CHILD EXTRACTION rather than a field read inline, which is what
//! lets its reasons reach the output through the ordinary walk instead of being threaded by hand.
//! It is also what lets this derive share a reader with the two trivial ones
//! (NOTE(#stage/one-reader-two-generators)) rather than reading the same attribute a third time."

use proc_macro_flow_traits::assert::Assert;
use proc_macro_flow_traits::extractor::{
    Extracted, Extraction, Extractor, Reason, ReasonKind, Validate,
};
use proc_macro_flow_traits::render::Diagnose;
use syn::{Data, DeriveInput, Expr, Field, Fields, FieldsNamed};

use super::super::ext::{AttributeExt, AttributesExt, FieldExt};
use super::super::stage::StageDeclaration;

/// A whole extraction type, as declared.
pub(crate) struct ExtractorExtraction<'ast> {
    /// `#[source(Ty)]` and the generics, read by the shared reader.
    pub(crate) declaration: Extracted<StageDeclaration<'ast>, &'ast DeriveInput>,
    /// One child per field. Every field of an extraction type IS a child by construction.
    pub(crate) fields: Vec<Extracted<ChildDeclaration<'ast>, &'ast Field>>,
}

/// One field's declaration: what it holds, and where it comes from.
pub(crate) struct ChildDeclaration<'ast> {
    pub(crate) ident: &'ast syn::Ident,
    pub(crate) ty: &'ast syn::Type,
    /// CARRIED, never interpreted - ID(no-parse). A bad expression is rustc's error at the
    /// author's own span once it is spliced.
    pub(crate) reach: Reach,
}

// TODO[x](#extractor/fields-may-hold-values): C[Attr(value)], "Attr(derive(Extractor)) accepted
// only fields that were child EXTRACTIONS, so a stage reading plain data off the AST could not use
// it at all - it hand-wrote extract_from and then owed two empty impls. A third head says the field
// holds a value; marking it apart rather than inferring it from the type keeps the `Extracted<T, I>`
// error where it belongs"
/// What a field holds, and how to get there.
///
/// NOTE(#extractor-derive/children-are-marked-not-inferred): V[E(Reach).V(Value).declared],
/// "Whether a field holds a CHILD EXTRACTION or a plain VALUE is DECLARED, never read off its type.
/// Inferring it - `Extracted<T, I>` means child, anything else means value - was considered and is
/// wrong twice over.
///
/// It would make a typo in the type silently change what the field MEANS, from 'descend into this
/// child' to 'assign this expression', with no error anywhere. And it would throw away the check
/// worth keeping: with the two marked apart, `#[from]` can still INSIST on `Extracted<T, I>` and
/// say so when it does not get one, which is the same error this derive has always given.
///
/// The type still decides everything it decided before - arity for a child comes off it and
/// nothing may contradict that (ID(from/arity-from-type)). What it does not decide is which
/// question is being asked of it"
pub(crate) enum Reach {
    /// `#[from(expr)]` - a CHILD, reached by an expression evaluated with `source` in scope.
    From(Expr),
    /// `#[with(callable)]` - a child, reached by applying a callable to `source`.
    With(Expr),
    /// `#[value(expr)]` - a plain value read straight off the source. Not an extraction, so the
    /// render walk does not descend into it.
    Value(Expr),
}

impl Reach {
    /// The expression, whichever way it was written.
    pub(crate) fn expr(&self) -> &Expr {
        match self {
            Reach::From(expr) | Reach::With(expr) | Reach::Value(expr) => expr,
        }
    }

    /// Whether this field holds an extraction the walk must descend into.
    pub(crate) fn is_child(&self) -> bool {
        !matches!(self, Reach::Value(_))
    }
}

impl<'ast> Validate<'ast> for ChildDeclaration<'ast> {
    type Source = &'ast Field;
    type Valid = &'ast Field;

    fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
        Ok(input)
    }
}

impl<'ast> Extractor<'ast> for ChildDeclaration<'ast> {
    type Output = Extracted<Self, &'ast Field>;

    fn extract_from(node: &'ast Field) -> Self::Output {
        fn read(node: &Field) -> Extraction<ChildDeclaration<'_>> {
            let ident = match node.named_ident() {
                Ok(ident) => ident,
                Err(error) => return Extraction::failed(Reason::new(ReasonKind::Syntax(error))),
            };

            let written = [
                node.attrs.find_one("from"),
                node.attrs.find_one("with"),
                node.attrs.find_one("value"),
            ];

            let mut found: Vec<(usize, &syn::Attribute)> = Vec::new();
            for (which, attr) in written.into_iter().enumerate() {
                match attr {
                    Ok(Some(attr)) => found.push((which, attr)),
                    Ok(None) => {}
                    Err(error) => {
                        return Extraction::failed(Reason::new(ReasonKind::Syntax(error)));
                    }
                }
            }

            // The three are ALTERNATIVES: one field, one answer to 'where does this come from'.
            let reach = match found.as_slice() {
                [(0, attr)] => attr.expr_arg().map(Reach::From),
                [(1, attr)] => attr.expr_arg().map(Reach::With),
                [(2, attr)] => attr.expr_arg().map(Reach::Value),
                [] => {
                    return Extraction::failed(Reason::at(ReasonKind::Missing, ident));
                }
                [_, (_, extra), ..] => {
                    return Extraction::failed(Reason::at(ReasonKind::Ambiguous, extra));
                }
                // `found` holds at most one entry per index, so a single entry is one of the
                // three above. Unreachable from any declaration that parsed.
                [(_, attr)] => {
                    return Extraction::failed(Reason::at(ReasonKind::WrongShape, attr));
                }
            };

            match reach {
                Ok(reach) => Extraction::value(ChildDeclaration {
                    ident,
                    ty: &node.ty,
                    reach,
                }),
                Err(error) => Extraction::failed(Reason::new(ReasonKind::Syntax(error))),
            }
        }

        Extracted::new(read(node), node)
    }
}

impl Assert for ChildDeclaration<'_> {}

impl Diagnose for ChildDeclaration<'_> {
    /// A leaf: its `reach` is carried tokens, not a child extraction.
    fn diagnose(&self, _: &mut Vec<syn::Error>) {}
}

impl<'ast> Validate<'ast> for ExtractorExtraction<'ast> {
    type Source = &'ast DeriveInput;
    /// A REAL narrowing, which is what this trait is for: an extraction type is a struct of named
    /// fields, and nothing else can be one.
    type Valid = &'ast FieldsNamed;

    fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
        let Data::Struct(data) = &input.data else {
            return Err(Reason::at(ReasonKind::WrongShape, &input.ident));
        };

        match &data.fields {
            Fields::Named(named) => Ok(named),
            // A tuple struct is all-positional, which cannot say where each child comes from.
            _ => Err(Reason::at(ReasonKind::WrongShape, &input.ident)),
        }
    }
}

impl<'ast> Extractor<'ast> for ExtractorExtraction<'ast> {
    type Output = Extracted<Self, &'ast DeriveInput>;

    fn extract_from(node: &'ast DeriveInput) -> Self::Output {
        let extraction = match Self::validate(node) {
            Ok(named) => Extraction::value(Self {
                declaration: StageDeclaration::extract_from(node),
                fields: ChildDeclaration::extract_each(named.named.iter()),
            }),
            Err(reason) => Extraction::failed(reason),
        };

        Extracted::new(extraction, node)
    }
}

impl Assert for ExtractorExtraction<'_> {}

impl Diagnose for ExtractorExtraction<'_> {
    fn diagnose(&self, out: &mut Vec<syn::Error>) {
        self.declaration.diagnose(out);
        self.fields.diagnose(out);
    }
}
