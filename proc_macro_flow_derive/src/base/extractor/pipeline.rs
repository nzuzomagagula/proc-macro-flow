// @review [ ]
//! The extractor stage's macro boundary.
//!
//! NOTE(#extractor/pipeline-exists-after-all): V[S(ExtractorPipeline).reason != ID(extractor/pipeline).reason],

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
