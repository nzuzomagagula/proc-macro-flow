use std::error::Error;

// @review [ ]
use crate::traits::extractor::Extractor;

pub(crate) mod extractor;
pub(crate) mod generator;
pub(crate) mod processor;
pub(crate) mod visitable;

// TODO(#pipeline/relocate-traits):M[Tr(Pipeline) => N(proc_macro_flow_traits)], "This crate has proc-macro = true, so pub items here (Pipeline, Validate, Extractor, and Processor/Generator once they exist) can't be depended on by any other crate - proc_macro_flow_traits is already an ordinary lib and already a dependency of this crate, so the base trait definitions belong there; StructExtraction/FieldExtraction/etc. here should just implement them"
pub trait Pipeline<'ast, E: Extractor<'ast>, P: Processor<'ast>, G: Generator<'ast>> {}

pub trait Validate<I> {
    //TODO[ ](#helper): U[this.Tr(Error), These should enforce better error management]
    type ValidityError;

    fn validate(&self, input: I) -> Result<I, Self::ValidityError>;
}
