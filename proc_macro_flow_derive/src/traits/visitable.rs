// @review [ ]
use syn::visit::Visit;
use syn::{Attribute, Field, Fields};

/// Binds a concrete `syn` AST node type to the `Visit` method that visits it,
/// so generic code can dispatch on the node's type instead of hardcoding the
/// `visit_*` method name at each call site.
pub(crate) trait Visitable<'ast> {
    fn accept<V: Visit<'ast> + ?Sized>(&'ast self, visitor: &mut V);
}

impl<'ast> Visitable<'ast> for Field {
    fn accept<V: Visit<'ast> + ?Sized>(&'ast self, visitor: &mut V) {
        visitor.visit_field(self);
    }
}

impl<'ast> Visitable<'ast> for Fields {
    fn accept<V: Visit<'ast> + ?Sized>(&'ast self, visitor: &mut V) {
        visitor.visit_fields(self);
    }
}

impl<'ast> Visitable<'ast> for Attribute {
    fn accept<V: Visit<'ast> + ?Sized>(&'ast self, visitor: &mut V) {
        visitor.visit_attribute(self);
    }
}
