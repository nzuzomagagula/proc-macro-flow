// @review [ ]
use crate::traits::visitable::Visitable;

#[derive(Default)]
pub enum ExtractionState<T> {
    Initialised(T),
    #[default]
    Uninitialised,
}

pub(crate) trait ExtractFrom<'ast>: Sized {
    type Node: Visitable<'ast> + ?Sized;

    fn extract_from(node: &'ast Self::Node) -> ExtractionState<Self>;
}

impl<T> ExtractionState<T> {
    pub(crate) fn extract<'ast>(node: &'ast T::Node) -> Self
    where
        T: ExtractFrom<'ast>,
    {
        T::extract_from(node)
    }
}
