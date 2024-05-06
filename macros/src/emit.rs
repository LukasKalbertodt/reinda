use std::{fmt, path::{Path, PathBuf}};
use glob::glob;

use proc_macro2::{Span, TokenStream};
use quote::quote;

use crate::{err, EmbedConfig, Error, Input};




pub(crate) fn emit(input: Input) -> Result<TokenStream, Error> {
    let config = input.with_defaults();

    // Figure out actual base path used for all paths below. We escape all glob
    // patterns in these, as these base paths should not be interpreted as glob
    // patterns. It would be nicer to give a base path to the glob walker API,
    // but that's not supported.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set");
    let manifest_dir = glob::Pattern::escape(&manifest_dir);
    let manifest_dir = Path::new(&manifest_dir);
    let base = match &config.base_path {
        Some(base_path) => manifest_dir.join(&glob::Pattern::escape(base_path)),
        None => PathBuf::from(manifest_dir),
    };

    let mut stats = Stats::default();
    let mut files = Vec::new();
    for (glob_pattern, span) in &config.files {
        let utf8_err = || err!(@span, "path is not valid UTF-8");

        // Construct full path.
        let full_path = base.join(glob_pattern).to_str().ok_or_else(utf8_err)?.to_owned();

        // Iterate over all files matching the glob pattern.
        let entries = glob(&full_path).map_err(|e| err!(@span, "invalid glob pattern: {e}"))?;
        for entry in entries {
            let file_path = entry
                .map_err(|e| err!(@span, "IO error while walking glob paths: {e}"))?;
            let short_path = file_path.strip_prefix(&base)
                .unwrap_or(&file_path)
                .to_str()
                .ok_or_else(utf8_err)?;
            let file_path = file_path.to_str().ok_or_else(utf8_err)?;

            // Load file the current build mode says so.
            let embed_tokens = embed(short_path, span, file_path, &config, &mut stats)?;

            files.push(quote! {
                reinda::EmbeddedFile {
                    #embed_tokens
                    glob: #glob_pattern,
                    path: #short_path,
                }
            });
        }
    }

    if config.print_stats {
        println!(
            "[reinda] Summary: embedded {} files ({} stored in compressed form), \
                totalling {} ({} when uncompressed)",
            stats.embedded_original + stats.embedded_compressed,
            stats.embedded_compressed,
            ByteSize(stats.compressed_size),
            ByteSize(stats.uncompressed_size),
        );
    }

    Ok(quote! {
        reinda::Embeds {
            files: &[ #(#files ,)* ],
        }
    })
}

#[derive(Default)]
struct Stats {
    uncompressed_size: usize,
    compressed_size: usize,
    embedded_original: u32,
    embedded_compressed: u32,
}

#[cfg(not(any(not(debug_assertions), feature = "always-embed")))]
fn embed(
    _: &str,
    _: &Span,
    _: &str,
    _: &EmbedConfig,
    _: &mut Stats,
) -> Result<TokenStream, Error> {

    Ok(quote!{})
}

#[cfg(any(not(debug_assertions), feature = "always-embed"))]
fn embed(
    path: &str,
    span: &Span,
    full_path: &str,
    config: &EmbedConfig,
    stats: &mut Stats,
) -> Result<TokenStream, Error> {
    // Read the full file.
    let data = std::fs::read(&full_path)
        .map_err(|e| err!(@span, "could not read '{full_path}': {e}"))?;
    stats.uncompressed_size += data.len();

    // Compress.
    let use_compressed_data: Option<Vec<u8>>;
    #[cfg(feature = "compress")]
    {
        let compression_threshold = config.compression_threshold;
        let compression_quality = config.compression_quality;

        let before = std::time::Instant::now();
        let mut compressed = Vec::new();
        brotli::BrotliCompress(&mut &*data, &mut compressed, &brotli::enc::BrotliEncoderParams {
            quality: compression_quality.into(),
            ..Default::default()
        }).expect("unexpected error while compressing");
        let compress_duration = before.elapsed();

        let compression_ratio = compressed.len() as f32 / data.len() as f32;
        let use_compression = compression_ratio < compression_threshold;
        if config.print_stats {
            println!(
                "[reinda] '{path}': compression ratio {:.1}% (original {}, compressed {}) \
                    => using {} (compression took {:.2?})",
                compression_ratio * 100.0,
                ByteSize(data.len()),
                ByteSize(compressed.len()),
                if use_compression { "compressed" } else { "original" },
                compress_duration,
            );
        }
        use_compressed_data = if use_compression { Some(compressed) } else { None };
    }
    #[cfg(not(feature = "compress"))]
    {
        use_compressed_data = None;
        if config.print_stats {
            println!("[reinda] '{path}': {}", ByteSize(data.len()));
        }
    }


    let content = if let Some(compressed) = &use_compressed_data {
        stats.compressed_size += compressed.len();
        stats.embedded_compressed += 1;
        let lit = proc_macro2::Literal::byte_string(compressed);
        quote! {
            {
                // This is to make cargo/the compiler understand that we
                // want to be recompiled if that file changes.
                include_bytes!(#full_path);

                #lit
            }
        }
    } else {
        stats.compressed_size += data.len();
        stats.embedded_original += 1;
        quote! {
            include_bytes!(#full_path)
        }
    };


    let compressed = use_compressed_data.is_some();
    Ok(quote! {
        content: #content,
        compressed: #compressed,
    })
}

struct ByteSize(usize);

impl fmt::Display for ByteSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 > 1500 * 1024 {
            write!(f, "{:.1}MiB", (self.0 / 1024) as f32 / 1024.0)
        } else if self.0 > 1500 {
            write!(f, "{:.1}KiB", self.0 as f32 / 1024.0)
        } else {
            write!(f, "{}B", self.0)
        }
    }
}
