// @review [~]
use crate::{
    StructExtraction,
    base::extractor::extractor::{attribute::TransformationExtraction, field::FieldExtraction},
};

pub struct ExtractorProcessor<'ast> {
    source: StructExtraction<'ast>,
}

pub struct FieldProcessor<'ast> {
    source: FieldExtraction<'ast>,
}

pub struct TransformationProcessor<'ast> {
    source: TransformationExtraction<'ast>,
}
