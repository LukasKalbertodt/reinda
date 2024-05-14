//! This library helps with managing assets (like JS or CSS files) in your web
//! application. It allows you to embed the files directly into your
//! executable (for ease of deployment), optionally in compressed form.
//! `reinda` can also insert a hash into filenames to facilitate powerful web
//! caching. In development mode, all files are loaded dynamically, avoiding
//! recompilation and thus reducing feedback cycles.
//!
//!
//! # Quick start
//!
//! There are three main steps:
//! 1. Use [`embed!`] to embed certain files into your binary.
//! 2. Configure your assets via [`Builder`] and build [`Assets`].
//! 3. Serve your assets via [`Assets::get`]
//!
//!
//! ```ignore
//! use reinda::Assets;
//!
//! // Step 1: specify assets that you want to embed into the executable.
//! const EMBEDS: reinda::Embeds = reinda::embed! {
//!     // Folder which contains your assets, relative to your `Cargo.toml`.
//!     base_path: "assets",
//!
//!     // List files to embed. Supports glob patterns
//!     files: [
//!         "index.html",
//!         "bundle.js",
//!         "icons/*.svg",
//!     ],
//! };
//!
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Step 2: Configure your assets next. In this example, all embedded
//!     // assets are just added, without any special configuration. Note though
//!     // that the first argument, the HTTP path, can differ.
//!     let mut builder = Assets::builder();
//!     builder.add_embedded("index.html", &EMBEDS["index.html"]);
//!     builder.add_embedded("static/main.js", &EMBEDS["bundle.js"]).with_hash();
//!     builder.add_embedded("static/icons/", &EMBEDS["icons/*.svg"]);
//!
//!     // You can also add assets not mentioned in `embed!` which are then
//!     // always loaded at runtime.
//!     builder.add_file("img/logo.svg", std::env::current_dir().unwrap().join("logo.svg"));
//!
//!     // Load & prepare all assets.
//!     let assets = builder.build().await?;
//!
//!
//!     // Step 3: serve assets (of course, this would be in an HTTP request handler).
//!     let asset = assets.get("index.html").unwrap();
//!     let bytes = asset.content().await?;
//!
//!     // ...
//!     Ok(())
//! }
//! ```
//!
//! For a longer and more practical example, see `examples/main.rs` in the
//! repository.
//!
//! In practice, you likely want to use [`EntryBuilder::with_hash`] for most of
//! your assets. And then use [`EntryBuilder::with_modifier`] and/or
//! [`EntryBuilder::with_path_fixup`] to fix the references across files.
//!
//! # Prod vs. dev mode
//!
//! Reinda operates in one of two modes: *prod* or *dev*. Prod mode is enabled
//! if you are building in release mode (e.g. `cargo build --release`) or if
//! you enabled the crate feature `always-prod`. Otherwise, dev mode is enabled.
//!
//! The mode influences the behavior of reinda significantly. The following
//! table describes those differences, though you likely don't need to worry
//! about the details too much.
//!
//! |                  | Prod               | Dev                   |
//! | ---------------- | ------------------ | --------------------- |
//! | **Summary**      | Embed assets & optimize for speed | Dynamically load assets for faster feedback cycles |
//! | `embed!`         | Loads & embeds assets into executable | Only stores asset paths |
//! | `with_hash`      | Hash inserted into filename | No hashes inserted |
//! | `Builder::build` | Loads all assets, applies modifiers, calculates hashes | Does hardly anything, keeps storing paths |
//! | `Assets::get`    | Hashmap lookup | Checks if path matches any assets or globs |
//! | `Asset::content` | Just returns the already loaded `Bytes` | Loads file from file system, applies modifier |
//!
//!
//! # Glossary: kinds of paths
//!
//! This library is dealing with different kind of paths, which could be
//! confused for one another. Here are the terms these docs try to use:
//!
//! - *FS path*: a proper path referring to one file on the file system.
//! - *Embed pattern*: what you specify in `files` inside `embed!`: could either
//!    be an FS path (referring to a single file) or contain a glob that
//!    matches any number of files.
//! - *HTTP path*: the path under which assets are reachable.
//!   - *unhashed HTTP path*: HTTP path before hashes are inserted. This is what
//!      you specify in all `Builder::add_*` methods.
//!   - *hashed HTTP path*: HTTP path after inserting hashes (if configured).
//!      This is what you pass to [`Assets::get`] and get inside
//!      [`Assets::iter`]. Even for assets without a hashed filename, the same
//!      term is used for consistency. Meaning: for non-hashed assets or in dev
//!      mode, the hashed and unhashed HTTP path is exactly the same.
//!
//!
//! # Cargo features
//!
//! - **`compress`** (enabled by default): if enabled, embedded files are
//!   compressed. This often noticably reduces the binary size of the
//!   executable. This feature adds the `brotli` dependency.
//!
//! - **`hash`** (enabled by default): is required for support of filename
//!   hashing (see above). This feature adds the `base64` and `sha2`
//!   dependencies.
//!
//! - **`always-prod`**: enabled *prod* mode even when compiled in debug mode.
//!   See the section about "prod" and "dev" mode above.
//!
//!
//! # Notes, Requirements and Limitations
//!
//! - `reinda` consists of two crates: `reinda-macros` and the main crate. Both
//!   crates have to be compiled with the same dev/prod mode. Cargo does this
//!   correctly by default, so don't worry about this unless you add
//!   per-dependency overrides.
//! - The environment variable `CARGO_MANIFEST_DIR` has to be set when expanding
//!   the `embed!` macro. Cargo does this automatically. But if you, for some
//!   reason, compile manually with `rustc`, you have to set that value.

#![deny(missing_debug_implementations)]

use std::{borrow::Cow, fmt, io, path::{Path, PathBuf}, sync::Arc};

use bytes::Bytes;

mod builder;
mod embed;
#[cfg(prod_mode)]
mod hash;
#[cfg(prod_mode)]
mod dep_graph;
pub mod util;

#[cfg_attr(prod_mode, path = "imp_prod.rs")]
#[cfg_attr(dev_mode, path = "imp_dev.rs")]
mod imp;



pub use self::{
    builder::{Builder, EntryBuilder},
    embed::{EmbeddedEntry, EmbeddedFile, EmbeddedGlob, Embeds},
};



/// Embeds files into the executable.
///
/// The syntax of this macro is similar to the struct initialization syntax:
///
/// ```ignore
/// const EMBEDS: reinda::Embeds = reinda::embed! {
///     base_path: "../frontend/build",
///     files: ["foo.txt", "bar.js", "icons/*.svg"],
/// };
/// ```
///
/// The following fields can be specified, with only `files` being mandatory:
///
/// - **`files`** (array of strings): list of paths or patterns of files that
///   should be embedded.
///
/// - **`base_path`** (string): a base path that is prefixed to all values in
///   `files`. Relative to `Cargo.toml`. Empty if unspecified. For a path `path`
///   in `files`, the following file is loaded:
///   `${CARGO_MANIFEST_DIR}/${base_dir}/${path}`.
///
/// - **`print_stats`** (bool): if set to true, reinda will print stats about
///   embedded files at compile time. Default: `false`.
///
/// - **`compression_threshold`** (float): number between 0 and 1 that
///   determines how well a file need to be compressible for it to be stored
///   in compressed form. A value of 0.7 would mean that a file is stored in
///   compressed form if that form is at most 70% as large as the original
///   file. There is a balance to strike: compression obviously reduces the
///   binary size, but also means that it has to be decompressed and that the
///   compressed and decompressed version will be in memory. Default: `0.85`.
///
/// - **`compression_quality`** (int): sets the Brotli compression quality (from
///   1 to 11). Default: `9`.
///
/// For compression to be used at all, the `compress` feature needs to be
/// enabled.
///
/// All entries in `files` falls in one of two categories. Either it's a plain
/// path without any (non-escaped) glob meta characters (`*?[]`), then the
/// resulting entry will be [`EmbeddedFile`]. Otherwise, if it contains glob
/// characters, it results in an [`EmbeddedGlob`]. Glob characters can be
/// escaped by surrounding them with `[]`, e.g. `a[*]-algorithm.txt`. For more
/// information on the supported glob patterns, see [the `glob` docs][glob].
///
///
/// [glob]: https://docs.rs/glob/latest/glob/struct.Pattern.html
pub use reinda_macros::embed;

/// Collection of assets, mapping from *hashed HTTP paths* to assets. Basically
/// a virtual file system.
///
/// This is the main type that you likely want to build once and then store for
/// the duration of your backend web application. In prod mode, this is
/// optimized to make serving assets as fast as possible. It's essentially a
/// `HashMap<String, Bytes>` in that case, with all file modifications already
/// applied.
///
/// You create an instance of this by using [`Self::builder`] and eventually
/// call [`Builder::build`].
#[derive(Debug, Clone)]
pub struct Assets(imp::AssetsInner);

impl Assets {
    /// Returns a builder, allowing you to add and configure assets.
    pub fn builder<'a>() -> Builder<'a> {
        Builder { assets: vec![] }
    }

    /// Retrieves an asset by *hashed HTTP path*. In prod mode, this is just a
    /// fast hash map lookup. In dev mode, the asset is loaded from the file
    /// system.
    pub fn get(&self, http_path: &str) -> Option<Asset> {
        self.0.get(http_path)
    }

    /// Returns the number of assets. For glob patterns, see [`Self::iter`] for
    /// details. This method always returns the same number as
    /// `self.iter().count()` (but faster).
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns an iterator over all assets and their *hashed HTTP paths*.
    ///
    /// *Note*: for assets included via glob pattern, this iterator only returns
    ///  those found at compile time. This does *not* perform a glob walk over
    ///  directories.
    pub fn iter(&self) -> impl '_ + Iterator<Item = (&str, Asset)> {
        self.0.iter()
    }
}


/// An fully prepared asset.
///
/// Very cheap to clone (in prod mode anyway, which is the only thing that
/// matters).
#[derive(Debug, Clone)]
pub struct Asset(imp::AssetInner);

impl Asset {
    /// Returns the contents of this asset. Will be loaded from the file system
    /// in dev mode, potentially returning IO errors. In prod mode, the file
    /// contents are already loaded and this method always returns `Ok(_)` and
    /// never yield.
    pub async fn content(&self) -> Result<Bytes, io::Error> {
        self.0.content().await
    }

    /// Returns whether this asset's filename contains a hash. Specifically, it
    /// returns true iff [`EntryBuilder::with_hash`] was called *and* you are
    /// compiling in prod mode.
    pub fn is_filename_hashed(&self) -> bool {
        self.0.is_filename_hashed()
    }
}

/// Passed to the modifier closure, e.g. allowing you to resolve *unhashed HTTP
/// paths* to *hashed ones*.
#[derive(Debug)]
pub struct ModifierContext<'a> {
    declared_deps: &'a [Cow<'static, str>],
    inner: imp::ModifierContextInner<'a>,
}

impl<'a> ModifierContext<'a> {
    /// Resolves an *unhashed HTTP path* to the *hashed HTTP path*.
    ///
    /// **Panics** if the passed `unhashed_http_path` was not declared as
    /// dependency in `with_modifier` or does not refer to an existing asset.
    pub fn resolve_path<'b>(&'b self, unhashed_http_path: &'b str) -> &'b str {
        if !self.declared_deps.iter().any(|dep| dep == unhashed_http_path) {
            panic!(
                "called `ModifierContext::resolve_path` with '{}', \
                    but that was not specified as dependency",
                unhashed_http_path,
            );
        }

        self.inner.resolve_path(unhashed_http_path).unwrap_or_else(|| {
            panic!(
                "called `ModifierContext::resolve_path` with '{}', \
                    but no asset with that path exists",
                unhashed_http_path,
            );
        })
    }

    /// Returns the dependencies you passed to [`EntryBuilder::with_modifier`],
    /// in the same order. This is just for convenience and to avoid cloning
    /// the dependency list.
    pub fn dependencies(&self) -> &'a [Cow<'static, str>] {
        self.declared_deps
    }
}

// =========================================================================================
// ===== Error
// =========================================================================================

/// Errors that might happen during [`Builder::build`], when loading and resolving files.
#[derive(Debug)]
#[non_exhaustive]
pub enum BuildError {
    Io {
        err: std::io::Error,
        path: PathBuf,
    },
    CyclicDependencies(Vec<String>),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Io { err, path }
                => write!(f, "IO error while accessing '{}': '{}'", path.display(), err),
            BuildError::CyclicDependencies(cycle) => write!(f, "cyclic dependencies: {:?}", cycle),
        }
    }
}

impl std::error::Error for BuildError {}



// =========================================================================================
// ===== Various types
// =========================================================================================

#[derive(Debug, Clone, Copy)]
#[cfg_attr(any(dev_mode, not(feature = "hash")), allow(dead_code))]
enum PathHash<'a> {
    None,
    Auto,
    InBetween {
        prefix: &'a str,
        suffix: &'a str,
    },
}

#[derive(Debug, Clone)]
enum DataSource {
    File(PathBuf),
    #[cfg_attr(dev_mode, allow(dead_code))]
    Loaded(Bytes),
}

impl DataSource {
    async fn load(&self) -> Result<Bytes, (io::Error, &Path)> {
        match self {
            DataSource::File(path) => tokio::fs::read(path).await
                .map(Into::into)
                .map_err(|err| (err, &**path)),
            DataSource::Loaded(bytes) => Ok(bytes.clone()),
        }
    }
}


#[derive(Clone)]
enum Modifier {
    None,
    #[cfg_attr(dev_mode, allow(dead_code))]
    PathFixup(Vec<Cow<'static, str>>),
    Custom {
        f: Arc<dyn Send + Sync + Fn(Bytes, ModifierContext) -> Bytes>,
        deps: Vec<Cow<'static, str>>,
    },
}

impl Modifier {
    #[cfg(prod_mode)]
    fn dependencies(&self) -> Option<&[Cow<'static, str>]> {
        match self {
            Modifier::None => None,
            Modifier::PathFixup(deps) => Some(deps),
            Modifier::Custom { deps, .. } => Some(deps),
        }
    }
}

impl std::fmt::Debug for Modifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Modifier::None => write!(f, "None"),
            Modifier::PathFixup(_) => write!(f, "PathFixup"),
            Modifier::Custom { .. } => write!(f, "Custom"),
        }
    }
}

/// A glob patttern split after all leading fixed path segments.
#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Debug, Clone)]
struct SplitGlob {
    /// All leading path segments from the glob that do not contain glob meta
    /// characters.
    prefix: &'static str,

    /// The second part of the glob, starting with a segment having glob meta
    /// characters.
    #[cfg_attr(prod_mode, allow(dead_code))]
    suffix: glob::Pattern,
}

impl SplitGlob {
    fn new(glob: &'static str) -> Self {
        let offset = Path::new(glob).components().find_map(|component| {
            let std::path::Component::Normal(seg) = component else {
                return None;
            };

            // We know it came from a `str` so this unwrap is fine.
            let seg = seg.to_str().unwrap();
            if seg.contains(&['*', '?', '[', ']']) {
                return Some(seg.as_ptr() as usize - glob.as_ptr() as usize);
            }

            None
        }).unwrap_or(glob.len());

        let (prefix, suffix) = glob.split_at(offset);

        Self {
            prefix,
            // The `expect` is fine as the glob was already parsed at compile time.
            suffix: glob::Pattern::new(suffix).expect("invalid glob"),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_glob() {
        macro_rules! check {
            ($whole:literal => $prefix:literal + $suffix:literal) => {
                assert_eq!(
                    SplitGlob::new($whole),
                    SplitGlob { prefix: $prefix, suffix: glob::Pattern::new($suffix).unwrap() },
                );
            };
        }

        check!("frontend/build/fonts/*.woff2" => "frontend/build/fonts/" + "*.woff2");
        check!("frontend/**/banana.txt" => "frontend/" + "**/banana.txt");
        check!("../foo/bar*/*.svg" => "../foo/" + "bar*/*.svg");
    }
}
