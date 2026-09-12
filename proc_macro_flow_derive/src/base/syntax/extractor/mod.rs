// @review [ ]

use proc_macro::Ident;
use syn::{Token, Type, punctuated::Punctuated};

pub struct SyntaxExtraction<'ast> {
    fields: Punctuated<SyntaxFieldExtraction<'ast>, Token![,]>,
}

pub struct SyntaxFieldExtraction<'ast> {
    attributes: SyntaxFieldAttributeExtraction,
    field_ident: &'ast Ident,
    field_ty: &'ast Type,
}

pub struct SyntaxFieldAttributeExtraction {
    k
}
