//! Types to describe the abstract parsed input.

use proc_macro2::Span;


#[derive(Debug)]
pub(crate) struct Input {
    pub(crate) base_path: Option<String>,
    pub(crate) compression_threshold: Option<f32>,
    pub(crate) compression_quality: Option<u8>,
    pub(crate) print_stats: Option<bool>,
    pub(crate) files: Vec<(String, Span)>,
}

impl Input {
    pub(crate) fn with_defaults(self) -> EmbedConfig {
        EmbedConfig {
            base_path: self.base_path,
            compression_threshold: self.compression_threshold.unwrap_or(0.9),
            compression_quality: self.compression_quality.unwrap_or(9),
            print_stats: self.print_stats.unwrap_or(false),
            files: self.files,
        }
    }
}

pub(crate) struct EmbedConfig {
    pub(crate) base_path: Option<String>,
    #[allow(dead_code)]
    pub(crate) compression_threshold: f32,
    #[allow(dead_code)]
    pub(crate) compression_quality: u8,
    pub(crate) print_stats: bool,
    pub(crate) files: Vec<(String, Span)>,
}
