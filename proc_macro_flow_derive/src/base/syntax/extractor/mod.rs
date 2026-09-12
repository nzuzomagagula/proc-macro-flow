// @review [ ]

use proc_macro::Ident;
use proc_macro2::TokenStream;
use syn::{Attribute, PathSegment, Token, Type, parse::Parse, punctuated::Punctuated};

use crate::traits::extractor::Extractor;

pub struct SyntaxExtraction<'ast> {
    fields: Vec<SyntaxFieldExtraction<'ast>>,
    alias: SyntaxAttributeExtraction<'ast>,
}

pub struct SyntaxAttributeExtraction<'ast> {
    alias: &'ast Ident,
}

pub struct SyntaxFieldExtraction<'ast> {
    attributes: Vec<SyntaxFieldAttributeExtraction<'ast>>,
    field_ident: &'ast Ident,
    field_ty: &'ast Type,
}

pub enum SyntaxFieldAttributeExtraction<'ast> {
    Shape(AttributeKind),
    Alias(&'ast Ident),
}

pub enum AttributeKind {
    MetaList,
    NamedValue,
    Path,
}

const ATTRIBUTE_KIND: Punctuated<PathSegment, Token![::]> =


impl<'ast> Extractor<'ast, &'ast TokenStream> for AttributeKind {
    //TODO[ ](#lazy):C[this]
    type ExtractionError;

    fn extract_from(node: &'ast &'ast TokenStream) -> Result<crate::ExtractionState<Self>, Self::ExtractionError> {
    //TODO[ ](#i-was-busy):U[this]
        let variant: AttributeKind = parse
    }
}

impl Extractor for SyntaxFieldAttributeExtraction {
    type Node;

    type ExtractionError;

    fn extract_from(
        node: &'ast Self::Node,
    ) -> Result<crate::ExtractionState<Self>, Self::ExtractionError> {
        todo!()
    }
}
