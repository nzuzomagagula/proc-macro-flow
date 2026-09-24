// @review [ ]
//! `#[derive(Generator)]`'s macro boundary.

use proc_macro_flow_traits::pipeline::Pipeline;

use super::extractor::GeneratorDeclaration;
use super::generator::GeneratorExpansion;

/// `#[derive(Generator)]`, wired.
// TODO[x](#generator/derive-is-a-pipeline): R[F(derive_generator) -> S(GeneratorWiring)], "Its
// validate is the newtype check - a generator IS a tuple struct, and what it wraps is what it
// emits (ID(generation/newtype-per-item))"
pub(crate) struct GeneratorWiring;

impl<'ast> Pipeline<'ast> for GeneratorWiring {
    type Extractor = GeneratorDeclaration<'ast>;
    type Processor = GeneratorDeclaration<'ast>;
    type Generator = GeneratorExpansion;
}
