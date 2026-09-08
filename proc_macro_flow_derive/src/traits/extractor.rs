// @review [ ]
use crate::traits::{Validate, visitable::Visitable};

#[derive(Default)]
pub enum ExtractionState<T> {
    Initialised(T),
    #[default]
    Uninitialised,
}

pub(crate) trait Extractor<'ast, I: Visitable<'ast>>: Sized + Validate<'ast, I> {
    type Node: Visitable<'ast> + ?Sized;
    type ExtractionError;

    fn extract_from(node: &'ast Self::Node)
    -> Result<ExtractionState<Self>, Self::ExtractionError>;
}

impl<T> ExtractionState<T> {
    pub(crate) fn extract<'ast>(node: &'ast T::Node) -> Self
    where
        T: Extractor<'ast>,
    {
        T::extract_from(node)
    }
}
