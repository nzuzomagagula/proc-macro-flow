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

/// The two ways a field says where its children are.
pub(crate) enum Reach {
    /// `#[from(expr)]` - an expression evaluated with `source` in scope.
    From(Expr),
    /// `#[with(callable)]` - applied to `source`.
    With(Expr),
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

            let (from, with) = match (node.attrs.find_one("from"), node.attrs.find_one("with")) {
                (Ok(from), Ok(with)) => (from, with),
                (Err(error), _) | (_, Err(error)) => {
                    return Extraction::failed(Reason::new(ReasonKind::Syntax(error)));
                }
            };

            let reach = match (from, with) {
                (Some(_), Some(other)) => {
                    return Extraction::failed(Reason::at(
                        ReasonKind::Ambiguous,
                        other,
                    ));
                }
                (Some(from), None) => from.expr_arg().map(Reach::From),
                (None, Some(with)) => with.expr_arg().map(Reach::With),
                (None, None) => {
                    return Extraction::failed(Reason::at(ReasonKind::Missing, ident));
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
