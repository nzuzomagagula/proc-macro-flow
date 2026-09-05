// @review [ ]
//TODO(#reader/scratch):C[B(scratch), "Create the scratch block to get the shape of things"]
// TODO(#reader/traits):C[MacDef(traits and stuff), "Start the crate creation of these thigs"]
// TODO[x](#reader/entry):C[Entry(DeriveInput -> StructExtraction), "Wire an entry point that drives ExtractionState<StructExtraction> from a DeriveInput/ItemStruct"]
pub mod attribute;

use syn::{Expr, Field, Meta, MetaNameValue, visit::Visit};

pub(crate) enum ExtractionState<T> {
    Initialised(T),
    Uninitialised,
}

impl<T> Default for ExtractionState<T> {
    fn default() -> Self {
        ExtractionState::Uninitialised
    }
}

pub(crate) struct StructExtraction<'ast> {
    fields: Vec<ExtractionState<FieldExtraction<'ast>>>,
}

struct FieldExtraction<'ast> {
    field: &'ast Field,
    transformation: Vec<ExtractionState<TransformationExtraction<'ast>>>,
}

struct TransformationExtraction<'ast> {
    expression: &'ast Expr,
}

impl<'ast> Visit<'ast> for ExtractionState<TransformationExtraction<'ast>> {
    fn visit_attribute(&mut self, i: &'ast syn::Attribute) {
        if let Meta::NameValue(mnv) = &i.meta {
            *self = ExtractionState::Initialised(TransformationExtraction {
                expression: &mnv.value,
            });
        }
    }
}

impl<'ast> Visit<'ast> for ExtractionState<FieldExtraction<'ast>> {
    fn visit_field(&mut self, i: &'ast syn::Field) {
        let transformation = i
            .attrs
            .iter()
            .map(|att| {
                let mut te = ExtractionState::Uninitialised;
                te.visit_attribute(att);
                te
            })
            .collect();
        *self = ExtractionState::Initialised(FieldExtraction {
            field: i,
            transformation,
        });
    }
}

impl<'ast> Visit<'ast> for ExtractionState<StructExtraction<'ast>> {
    fn visit_fields(&mut self, i: &'ast syn::Fields) {
        let fields = i
            .iter()
            .map(|field| {
                let mut fe = ExtractionState::Uninitialised;
                fe.visit_field(field);
                fe
            })
            .collect();
        *self = ExtractionState::Initialised(StructExtraction { fields });
    }
}

mod scratch {}
