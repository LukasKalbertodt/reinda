use std::path::{Path, PathBuf};
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
    let manifest_dir = Path::new(&manifest_dir);
    let base = match &config.base_path {
        Some(base_path) => manifest_dir.join(&base_path),
        None => PathBuf::from(manifest_dir),
    };
    let base_str = base.to_str()
        .ok_or_else(|| err!("base path or CARGO_MANIFEST_DIR is not valid UTF-8"))?;
    let escaped_base = glob::Pattern::escape(&base_str);
    let escaped_base = Path::new(&escaped_base);

    let mut stats = Stats::default();
    let mut entries = Vec::new();
    for (path, span) in &config.files {
        let utf8_err = || err!(@span, "path is not valid UTF-8");

        match Globness::check(path) {
            Globness::NotGlob(unescaped) => {
                let full_path = base.join(&unescaped).to_str().ok_or_else(utf8_err)?.to_owned();
                let embed_tokens = embed(&unescaped, span, &full_path, &config, &mut stats)?;

                entries.push(quote! {
                    reinda::EmbeddedEntry::Single(
                        reinda::EmbeddedFile {
                            #embed_tokens
                            path: #unescaped,
                        }
                    )
                });
            }

            Globness::Glob => {
                // Construct full path.
                let full_path = escaped_base.join(path).to_str().ok_or_else(utf8_err)?.to_owned();

                // Iterate over all files matching the glob pattern.
                let glob_walker = glob(&full_path)
                    .map_err(|e| err!(@span, "invalid glob pattern: {e}"))?;
                let mut files = Vec::new();
                for entry in glob_walker {
                    let file_path = entry
                        .map_err(|e| err!(@span, "IO error while walking glob paths: {e}"))?;
                    // skip directories
                    if file_path.is_dir() {
                        continue;
                    }
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
                            path: #short_path,
                        }
                    });
                }

                let base_path_tokens = if cfg!(prod_mode) {
                    quote! {}
                } else {
                    quote! {
                        base_path: #base_str,
                    }
                };

                entries.push(quote! {
                    reinda::EmbeddedEntry::Glob(reinda::EmbeddedGlob {
                        pattern: #path,
                        #base_path_tokens
                        files: &[ #(#files ,)* ],
                    })
                });
            }
        }
    }

    if config.print_stats {
        #[cfg(prod_mode)]
        println!(
            "[reinda] Summary: embedded {} files ({} stored in compressed form), \
                totalling {} ({} when uncompressed)",
            stats.embedded_original + stats.embedded_compressed,
            stats.embedded_compressed,
            ByteSize(stats.compressed_size),
            ByteSize(stats.uncompressed_size),
        );

        #[cfg(dev_mode)]
        println!("[reinda] Summary: in dev mode -> no files embedded");
    }



    Ok(quote! {
        reinda::Embeds {
            entries: &[ #(#entries ,)* ],
        }
    })
}

#[cfg_attr(test, derive(PartialEq, Debug))]
enum Globness {
    NotGlob(String),
    Glob,
}

impl Globness {
    fn check(s: &str) -> Self {
        let mut unescaped = String::new();
        let mut offset = 0;
        while let Some(i) = s[offset..].find(&['?', '*', '[', ']']) {
            // Push the preceeding uninteresting part to the output string.
            unescaped.push_str(&s[offset..][..i]);

            // We found a meta character. The only way the input string isn't a
            // glob is if this is a start of a simple escaped meta character.
            // In that case, we undo the escaping.
            match () {
                () if s[offset + i..].starts_with("[?]") => unescaped.push('?'),
                () if s[offset + i..].starts_with("[*]") => unescaped.push('*'),
                () if s[offset + i..].starts_with("[]]") => unescaped.push(']'),
                () if s[offset + i..].starts_with("[[]") => unescaped.push('['),
                _ => return Self::Glob,
            }

            // The only way we reach this if we encountered the escaped meta
            // character, so we advance by 3.
            offset += i + 3;
        }

        // Push the rest.
        unescaped.push_str(&s[offset..]);

        // We have not encountered meta characters, except simple escapes.
        Self::NotGlob(unescaped)
    }
}


#[derive(Default)]
#[allow(dead_code)]
struct Stats {
    uncompressed_size: usize,
    compressed_size: usize,
    embedded_original: u32,
    embedded_compressed: u32,
}

#[cfg(dev_mode)]
fn embed(
    _: &str,
    _: &Span,
    full_path: &str,
    _: &EmbedConfig,
    _: &mut Stats,
) -> Result<TokenStream, Error> {
    Ok(quote! {
        full_path: #full_path,
    })
}

#[cfg(prod_mode)]
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

#[cfg(prod_mode)]
struct ByteSize(usize);

#[cfg(prod_mode)]
impl std::fmt::Display for ByteSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 > 1500 * 1024 {
            write!(f, "{:.1}MiB", (self.0 / 1024) as f32 / 1024.0)
        } else if self.0 > 1500 {
            write!(f, "{:.1}KiB", self.0 as f32 / 1024.0)
        } else {
            write!(f, "{}B", self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Globness;

    #[test]
    fn glob_classification() {
        assert_eq!(Globness::check("foo.txt"), Globness::NotGlob("foo.txt".into()));
        assert_eq!(Globness::check("bar/foo.txt"), Globness::NotGlob("bar/foo.txt".into()));
        assert_eq!(Globness::check("fo[?]x.svg"), Globness::NotGlob("fo?x.svg".into()));
        assert_eq!(Globness::check("fo[*]x.svg"), Globness::NotGlob("fo*x.svg".into()));
        assert_eq!(Globness::check("fo[]]x.svg"), Globness::NotGlob("fo]x.svg".into()));
        assert_eq!(Globness::check("fo[[]x.svg"), Globness::NotGlob("fo[x.svg".into()));

        assert_eq!(Globness::check("fo*x.svg"), Globness::Glob);
        assert_eq!(Globness::check("fo?x.svg"), Globness::Glob);
        assert_eq!(Globness::check("fo[ab]x.svg"), Globness::Glob);
    }
}
