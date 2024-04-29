//! Types to describe the abstract parsed input.

use proc_macro2::Span;


#[derive(Debug)]
pub(crate) struct Input {
    pub(crate) base_path: Option<String>,
    #[allow(dead_code)]
    pub(crate) compression_threshold: Option<f32>,
    #[allow(dead_code)]
    pub(crate) compression_quality: Option<u8>,
    #[allow(dead_code)]
    pub(crate) print_stats: Option<bool>,
    pub(crate) files: Vec<(String, Span)>,
}
