#[derive(Default)]
pub enum ExtractionState<T> {
    Initialised(T),
    #[default]
    Uninitialised,
}

pub trait Extractor {
    type Source:
}
