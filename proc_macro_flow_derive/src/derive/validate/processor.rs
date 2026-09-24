// @review [ ]
//! What this derive understands: the same type again.
//!
//! Naming the extractor as the processor is ID(pipeline/no-processor-is-the-extractor), and here it
//! is not a placeholder - ID(stage/lifetime-is-the-processing) describes the real, small thing
//! this stage computes. See ID(derive/shared-stages-are-re-exported) for why this is a re-export.

pub(crate) use super::super::stage::ProcessedStage;
