// @review [ ]
//TODO(#extractor/scratch):C[B(scratch), "Create the scratch block to get the shape of things"]
// TODO(#extractor/traits):C[MacDef(traits and stuff), "Start the crate creation of these thigs"]
// TODO[x](#extractor/entry):C[Entry(DeriveInput -> StructExtraction), "Wire an entry point that drives ExtractionState<StructExtraction> from a DeriveInput/ItemStruct"]
pub mod extractor;
pub mod generator;
pub mod processor;

pub(crate) use extractor::{ExtractionState, StructExtraction};
