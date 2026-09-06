// @review [ ]
use crate::traits::{Validate, visitable::Visitable};

#[derive(Default)]
pub enum ExtractionState<T> {
    Initialised(T),
    #[default]
    Uninitialised,
}

pub(crate) trait Extractor<'ast>: Sized + Validate {
    type Node: Visitable<'ast> + ?Sized;

    fn extract_from(node: &'ast Self::Node) -> ExtractionState<Self>;
}

impl<T> ExtractionState<T> {
    pub(crate) fn extract<'ast>(node: &'ast T::Node) -> Self
    where
        T: Extractor<'ast>,
    {
        T::extract_from(node)
    }
}
