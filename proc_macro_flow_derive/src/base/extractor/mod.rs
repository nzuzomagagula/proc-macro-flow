// @review [ ]
//TODO[x](#extractor/scratch):C[N(scratch)], "Create the scratch block to get the shape of things"
// DEPRECATED(#extractor/traits):D[this] && V[ID(pipeline/relocate-traits) ==? this], "Was 'Start the crate creation of these thigs' with a MacDef(traits and stuff) selector that names no node. The real work is ID(pipeline/relocate-traits) - move the base traits into proc_macro_flow_traits - and it is tracked there. Nothing is left for this identity"
// TODO[x](#extractor/entry):C[F(extractor).A(\1).T(DeriveInput)], "Wire an entry point that drives ExtractionState<StructExtraction> from a DeriveInput/ItemStruct. Selector was Entry(..), which is not in the language - the node it meant is lib.rs::extractor's first argument"
pub mod extractor;
pub mod generator;
pub mod processor;

pub(crate) use extractor::{ExtractionState, StructExtraction};
