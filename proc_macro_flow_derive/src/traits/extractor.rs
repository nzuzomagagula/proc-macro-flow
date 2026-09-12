// @review [ ]
use crate::traits::{Validate, visitable::Visitable};

// TODO[x](#cleanup):R[E(ExtractionState) -> S(Extraction)], "RESOLVED, but not as a typestate. The answer is { value: Option<T>, reasons: Vec<Spanned<Reason>> } - see #syntax/extraction. A typestate cannot express 'this node extracted fine AND has a complaint of its own', which is what an unknown key is: a failure of the PARENT to consume its input, with the value still perfectly good. Two states could not carry a reason at all"
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
