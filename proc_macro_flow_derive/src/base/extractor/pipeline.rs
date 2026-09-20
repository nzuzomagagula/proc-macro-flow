// @review [ ]
//! The extractor stage's macro boundary.
//!
//! NOTE(#extractor/pipeline-exists-after-all): V[S(ExtractorPipeline).reason != ID(extractor/pipeline).reason],
//! "This type carries the name that Answer(#extractor/self-hosting) said nothing would need, and
//! that is not a reversal - it is a narrower claim surviving a wider one. The old
//! ID(extractor/pipeline) wanted a struct holding an extractor/processor/generator triple in order
//! to DESCRIBE the extraction so a macro could expand it, and that really is redundant: the field
//! type carries the arity and Ty(Extracted) carries the source, so the description would restate
//! what the extraction struct already says. The derive replaced it.
//!
//! What this holds the triple FOR is different: naming the three stages is how the bounds make them
//! agree, and the reason it exists at all is to own NORMALISATION
//! (ID(pipeline/owns-normalisation)). The old warning is the guard rail - if this type ever grows a
//! field or a method that restates the extraction, that is the drift the answer was right about"

use proc_macro_flow_traits::pipeline::Pipeline;

use crate::base::extractor::extractor::StructExtraction;
use crate::base::extractor::processor::ProcessedStruct;

pub(crate) struct ExtractorPipeline;

impl<'ast> Pipeline<'ast> for ExtractorPipeline {
    type Extractor = StructExtraction<'ast>;

    /// The SAME type as the extractor, and that is the whole of
    /// NOTE(#pipeline/no-processor-is-the-extractor): this pipeline has real processing to do
    /// (narrowing to ProcessedStruct), and it happens to be declared on the extraction type, so
    /// naming it twice is honest rather than a placeholder.
    type Processor = StructExtraction<'ast>;

    type Generator = ProcessedStruct<'ast>;
}
