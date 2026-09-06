// @review [ ]
use syn::parse::Parse;
use syn::visit::Visit;

use crate::traits::visitable::Visitable;

#[derive(Default)]
pub enum ExtractionState<T> {
    Initialised(T),
    #[default]
    Uninitialised,
}

impl<T> ExtractionState<T> {
    /// Runs the `Visit` impl bound to `Self` against `node`, dispatching through
    /// `node`'s `Visitable` impl instead of hardcoding a `visit_*` method name.
    pub(crate) fn extract<'ast, N>(node: &'ast N) -> Self
    where
        N: Visitable<'ast> + ?Sized,
        Self: Visit<'ast> + Default,
    {
        let mut state = Self::default();
        node.accept(&mut state);
        state
    }
}

pub trait Extractor {
    type Source: Parse;

    fn new(source: &Self::Source) -> Self;
}
