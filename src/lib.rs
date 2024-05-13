//! This library helps with easily including and serving assets (like JS or CSS
//! files) in your web application. It is fairly configurable and supports a
//! variety of features. In particular, it can embed all assets into your
//! executable at compile time to get an easy to deploy standalone-executable.
//!
//!
//! # Quick start
//!
//! To use `reinda`, you mostly need to do three things: (1) define your assets
//! with [`assets!`], (2) create an [`Assets`] instance, (3) call
//! [`Assets::get`] to serve your asset.
//!
//! ```ignore
//! use reinda::{assets, Assets, Config, Setup};
//!
//! const ASSETS: Setup = assets! {
//!     // Folder which contains your assets, relative to your `Cargo.toml`.
//!     #![base_path = "assets"]
//!
//!     // List of assets to include, with different settings.
//!     "index.html": { template },
//!     "bundle.js": { hash },
//! };
//!
//!
//! #[tokio::main]
//! async fn main() -> Result<(), reinda::Error> {
//!     // Initialize assets
//!     let assets = Assets::new(ASSETS, Config::default()).await?;
//!
//!     // Retrieve specific asset. You can now send this data via HTTP or use
//!     // it however you like.
//!     let bytes /*: Option<bytes::Bytes> */ = assets.get("index.html").await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! The `hash` keyword in the macro invocation means that `bundle.js` will be
//! obtainable only with a filename that contains a hash of its content, e.g.
//! `bundle.JdeK1YeQ90aJ.js`. This is useful for caching on the web: you can now
//! serve the `bundle.js` with a very large `max-age` in the `cache-control`
//! header. Whenever your asset changes, the URI changes as well, so the browser
//! has to re-request it.
//!
//! But how do you include the correct JS bundle path in your HTML file? That's
//! what `template` is for. `reinda` supports very basic templating. If you
//! define your HTML file like this:
//!
//! ```text
//! <html>
//!   <head></head>
//!   <body>
//!     <script src="{{: path:bundle.js :}}" />
//!   </body>
//! </html>
//! ```
//!
//! Then the `{{: ... :}}` part will be replaced by the actual, hashed path of
//! `bundle.js`. There are more uses for the template, as you can see below.
//!
//! To learn more about this library, keep reading, or check out the docs for
//! [`assets!`] for information on asset specification, or checkout [`Config`]
//! for information about runtime configuration.
//!
//!
//! # Embed or loaded at runtime: dev vs. prod mode
//!
//! This library has two different modes: **dev** (short for development) and
//! **prod** (short for production). The name "prod" is deliberately not the
//! same as "release" (the typical Rust term for it), because those two are not
//! always the same.
//!
//! There are several differences between the two modes:
//!
//! |   | dev mode | prod mode |
//! | - | -------- | --------- |
//! | Normal assets | Loaded from filesystem when requested | Embedded into binary |
//! | `dynamic: true` assets | Loaded from filesystem when requested | Loaded in [`Assets::new`] |
//! | `hash: true` assets | Filename not modified | Hash inserted into filename |
//! | Base path | `config.base_path` with current workdir as fallback | Given via `#![base_path]` |
//!
//!
//! By default, if you compile in Cargo debug mode (e.g. `cargo build`), *dev*
//! mode is used. If you compile in Cargo's release mode (e.g. `cargo build
//! --release`), *prod* mode is used. You can instruct `reinda` to always use
//! prod mode by enabling the feature `debug-is-prod`:
//!
//! ```text
//! reinda = { version = "...", features = ["debug-is-prod"] }
//! ```
//!
//!
//! # Template
//!
//! `reinda` has a simple template system. The input file is checked for
//! *fragments* which have the syntax `{{: foo :}}`. The start token is actually
//! `{{: ` (note the whitespace!). So `{{:foo:}}` is not recognized as fragment.
//! The syntax was chosen to not conflict with other template syntax that might
//! be present in the asset files. Please let me know if some other template
//! engine out there uses the `{{:` syntax! Then I might change the syntax of
//! `reinda`.
//!
//! Inside a fragment, there are different replacement functions you can use:
//!
//! - **`include:`** allows you to include the content of another file in place
//!   of the template fragment. If the included file is a template as well, that
//!   will be rendered before being included. Example:
//!   `{{: include:colors.css }}`.
//!
//! - **`path:`** replaces the fragment with the potential hashed path of
//!   another asset. This only makes sense for hashed asset paths as otherwise
//!   you could just insert the path directly. Example:
//!   `{{: path:bundle.js :}}`.
//!
//! - **`var:`** replaces the fragment with a runtime provided variable. See
//!   [`Config::variables`]. Example: `{{: var:main-color :}}`.
//!
//! Fragments have two other intended limitations: they must not contain a
//! newline and must not be longer than 256 characters. This is to further
//! prevent the template accidentally picking up start tokens that are not
//! intended for `reinda`.
//!
//!
//! ### Example
//!
//! **`index.html`**:
//!
//! ```text
//! <html>
//!   <head>
//!     <script type="application/json">{ "accentColor": "{{: var:color :}}" }</script>
//!     <style>{{: include:style.css :}}</style>
//!   </head>
//!   <body>
//!     <script src="{{: path:bundle.js :}}" />
//!   </body>
//! </html>
//! ```
//!
//! **`style.css`**
//!
//! ```text
//! body {
//!   margin: 0;
//! }
//! ```
//!
//! And assuming `bundle.js` was declared with `hash` (to hash its filename) and
//! the `config.variables` contained the entry `"color": "blue"`, then the
//! resulting `index.html` looks like this:
//!
//! ```text
//! <html>
//!   <head>
//!     <script type="application/json">{ "accentColor": "blue" }</script>
//!     <style>body {
//!   margin: 0;
//! }</style>
//!   </head>
//!   <body>
//!     <script src="bundle.JdeK1YeQ90aJ.js" />
//!   </body>
//! </html>
//! ```
//!
//!
//! # Cargo features
//!
//! - **`compress`** (enabled by default): if enabled, embedded files are
//!   compressed. This often noticably reduces the binary size of the
//!   executable. This feature adds the `flate2` dependency.
//!
//! - **`hash`** (enabled by default): is required for support of filename
//!   hashing (see above). This feature adds the `base64` and `sha2`
//!   dependencies.
//!
//! - **`debug-is-prod`**: see the section about "prod" and "dev" mode above.
//!
//!
//! # Notes, Requirements and Limitations
//!
//! - `reinda` actually consists of three crates: `reinda-core`, `reinda-macros`
//!   and the main crate. To detect whether Cargo compiles in debug or release
//!   mode, `cfg(debug_assertions)` is used. All three of these crates have to
//!   be compiled with with the same setting regarding debug assertions,
//!   otherwise you will either see strange compile errors or strange runtime
//!   behavior (no UB though). This shouldn't be a concern, as all crates in
//!   your dependency graph are compiled with the same codegen settings, unless
//!   you include per-dependency overrides in your `Cargo.toml`. So just don't
//!   do that.
//! - The environment variable `CARGO_MANIFEST_DIR` has to be set when expanding
//!   the `assets!` macro. Cargo does this automatically. But if you, for some
//!   reason, compile manually with `rustc`, you have to set that value.

// TODO
// #![deny(missing_debug_implementations)]

use std::{borrow::Cow, fmt, io, path::{Path, PathBuf}, sync::Arc};

use bytes::Bytes;

pub mod builder;
pub mod embed;
#[cfg(prod_mode)]
pub mod hash;
#[cfg(prod_mode)]
mod dep_graph;

#[cfg_attr(prod_mode, path = "imp_prod.rs")]
#[cfg_attr(dev_mode, path = "imp_dev.rs")]
mod imp;



pub use self::{
    builder::{Builder, EntryBuilder},
    embed::{EmbeddedEntry, EmbeddedFile, EmbeddedGlob, Embeds, embed},
};


/// A collection of assets, defined as a map from HTTP path to asset. Basically
/// a virtual file system.
///
/// TODO: explain more
pub struct Assets(imp::AssetsInner);

impl Assets {
    pub fn builder<'a>() -> Builder<'a> {
        Builder { assets: vec![] }
    }

    pub fn get(&self, http_path: &str) -> Option<Asset> {
        self.0.get(http_path)
    }

    /// Returns the number of assets. For glob patterns, see [`Self::iter`] for
    /// details. This method always returns the same number as `self.iter
    /// ().count()` (only faster).
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns an iterator over all assets. *Note*: for assets included via
    /// glob pattern, this iterator only returns those found at compile time.
    /// This does *not* perform a glob walk over directories.
    pub fn iter(&self) -> impl '_ + Iterator<Item = Asset> {
        self.0.iter()
    }
}


/// An asset.
///
/// Very cheap to clone (in prod mode anyway, which is the only thing that
/// matters).
#[derive(Clone)]
pub struct Asset(imp::AssetInner);

impl Asset {
    /// Returns the contents of this asset. Will be loaded from the file system
    /// in dev mode, potentially returning IO errors. In prod mode, the file
    /// contents are already loaded and this method always returns `Ok(_)`.
    pub async fn content(&self) -> Result<Bytes, io::Error> {
        self.0.content().await
    }

    pub fn is_filename_hashed(&self) -> bool {
        self.0.is_filename_hashed()
    }
}


pub struct ModifierContext<'a> {
    declared_deps: &'a [Cow<'static, str>],
    inner: imp::ModifierContextInner<'a>,
}

impl<'a> ModifierContext<'a> {
    /// Resolves to the actual asset HTTP path, including hash if configured
    /// that way.
    ///
    /// **Panics** if the passed `unhashed_http_path` was not declared as
    /// dependency in `with_modifier` and refers to an existing asset.
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
}

// =========================================================================================
// ===== Error
// =========================================================================================

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
    Custom {
        f: Arc<dyn Send + Sync + Fn(Bytes, ModifierContext) -> Bytes>,
        deps: Vec<Cow<'static, str>>,
    },
}

impl std::fmt::Debug for Modifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Modifier::None => write!(f, "None"),
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
