use std::path::Path;

use proc_macro2::{Span, TokenStream};
use quote::quote;

use crate::{err, Error, Input};


pub(crate) fn emit(input: Input) -> Result<TokenStream, Error> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .expect("CARGO_MANIFEST_DIR not set");
    let manifest_dir = Path::new(&manifest_dir);

    let files = input.files.iter()
        .map(|(path, span)| {
            // Construct full path.
            let full_path = match &input.base_path {
                Some(base_path) => manifest_dir.join(base_path).join(path),
                None => manifest_dir.join(path),
            };
            let full_path = full_path.to_str()
                .ok_or_else(|| err!(@span, "path is not valid UTF-8"))?
                .to_owned();

            // Load file the current build mode says so.
            let embed_tokens = embed(path, span, &full_path, &input)?;

            Ok(quote! {
                reinda::EmbeddedFile {
                    #embed_tokens
                    path: #path,
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(quote! {
        reinda::Embeds {
            files: &[ #(#files ,)* ],
        }
    })
}

#[cfg(not(any(not(debug_assertions), feature = "always-embed")))]
fn embed(
    _: &str,
    _: &Span,
    _: &str,
    _: &Input,
) -> Result<TokenStream, Error> {
    Ok(quote!{})
}

#[cfg(any(not(debug_assertions), feature = "always-embed"))]
fn embed(
    path: &str,
    span: &Span,
    full_path: &str,
    input: &Input,
) -> Result<TokenStream, Error> {
    let print_stats = input.print_stats.unwrap_or(false);

    // Read the full file.
    let data = std::fs::read(&full_path)
        .map_err(|e| err!(@span, "could not read '{full_path}': {e}"))?;


    // Compress.
    let use_compressed_data: Option<Vec<u8>>;
    #[cfg(feature = "compress")]
    {
        let compression_threshold = input.compression_threshold.unwrap_or(0.9);
        let compression_quality = input.compression_quality.unwrap_or(9);

        let before = std::time::Instant::now();
        let mut compressed = Vec::new();
        brotli::BrotliCompress(&mut &*data, &mut compressed, &brotli::enc::BrotliEncoderParams {
            quality: compression_quality.into(),
            ..Default::default()
        }).expect("unexpected error while compressing");
        let compress_duration = before.elapsed();

        let compression_ratio = compressed.len() as f32 / data.len() as f32;
        let use_compression = compression_ratio < compression_threshold;
        if print_stats {
            println!(
                "[reinda] '{path}': compression ratio {:.1}% (original {}, compressed {}) \
                    => using {} (compression took {:.2?})",
                compression_ratio * 100.0,
                data.len(),
                compressed.len(),
                if use_compression { "compressed" } else { "original" },
                compress_duration,
            );
        }
        use_compressed_data = if use_compression { Some(compressed) } else { None };
    }
    #[cfg(not(feature = "compress"))]
    {
        use_compressed_data = None;
        if print_stats {
            println!("[reinda] '{path}': {} bytes", data.len());
        }
    }


    let content = if let Some(compressed) = &use_compressed_data {
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
