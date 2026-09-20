// @review [ ]
//TODO[x](#extractor/scratch):C[N(scratch)], "Create the scratch block to get the shape of things"
// DEPRECATED(#extractor/traits):D[this] && V[ID(pipeline/relocate-traits) ==? this], "Was 'Start the crate creation of these thigs' with a MacDef(traits and stuff) selector that names no node. The real work is ID(pipeline/relocate-traits) - move the base traits into proc_macro_flow_traits - and it is tracked there. Nothing is left for this identity"
// TODO[x](#extractor/entry):C[F(extractor).A(\1).T(DeriveInput)], "DONE. lib.rs::field_names (called lib.rs::extractor when this was written - see ID(extractor/macro-wiring)) parses a DeriveInput and calls StructExtraction::extract_from, and - since the last pass - actually READS the result rather than discarding it. Two corrections to the original wording: the type is Extracted<StructExtraction, &DeriveInput> and not ExtractionState, which no longer exists; and the traversal is no longer a Visit walk, because extract_from descends from the DeriveInput itself (ID(extractor/recursive-source))"
pub mod extractor;
pub mod generator;
pub mod pipeline;
pub mod processor;

