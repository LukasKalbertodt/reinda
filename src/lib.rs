//! This library helps with easily including and serving assets (like JS or CSS
//! files) in your web application. It is fairly configurable and supports a
//! variety of features. In particular, it can embed all assets into your
//! executable at compile to get an easy to deploy standalone-executable.
//!
//! # Quick start
//!
//! To use `reinda`, you mostly need to do three things: (1) define your assets
//! with [`assets!`], (2) create an [`Assets`] instance, (3) call
//! [`Assets::get`] to serve your asset.
//!
//! ```no_run
//! use reinda::{assets, Assets, Config, Setup};
//!
//! const ASSETS: Setup = assets! {
//!     // Folder which contains your assets, relative to your `Cargo.toml`.
//!     #![base_path = "assets"]
//!
//!     // List of assets to include, with different settings.
//!     "index.html": { template },
//!     "bundle.js": { hash },
//! }
//!
//! // Initialize assets
//! let assets = Assets::new(ASSETS, Config::default());
//!
//! // Retrieve specific asset. You can now send this data via HTTP.
//! let bytes = assets.get("index.html");
//! ```
//!
//! The `hash` keyword in the macro invocation means that `bundle.js` will be
//! obtainable only with a filename that contains a hash of its content, e.g.
//! `whS3Hn7q-bundle.js`. This is useful for caching on the web: you can now
//! serve the `bundle.js` with a very large `max-age` in the `cache-control`
//! header. Whenever your asset changes, the URI changes as well, so the browser
//! has to re-request it.
//!
//! But how do you include the correct JS bundle path in your HTML file? That's
//! what `template` is for. `reinda` supports a very basic templating. If you
//! define your HTML file like this:
//!
//! ```text
//! <html>
//!   <head>
//!     <script type="application/javascript" src="{{: path:bundle.js :}}" />
//!   </head>
//!   <body></body>
//! </html>
//! ```
//!
//! Then the `{{: ... :}}` part will be replaced by the actual, hashed path of
//! `bundle.js`. There are more uses for the template, as you can see below.
//!
//!
//! # Embed or not to embed: dev vs. prod mode
//!
//! TODO
//!
//!
//! # Asset specification with `assets!`
//!
//! TODO
//!
//!
//! # Template
//!
//! TODO
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

#![deny(missing_debug_implementations)]

use std::{collections::HashMap, path::PathBuf};
use bytes::Bytes;

#[cfg(not(debug_assertions))]
use ahash::AHashMap;

use reinda_core::template;
use crate::resolve::Resolver;

mod dep_graph;
mod hash;
mod resolve;


/// Compile time configuration of assets. Returns a [`Setup`].
///
/// Simple example:
///
/// ```
/// use reinda::{assets, Setup};
///
/// const ASSETS: Setup = assets! {
///     #![base_path = "frontend/build"]
///
///     "index.html": { template },
///     "bundle.js": { hash },
/// };
/// ```
///
///
/// # Syntax
///
/// The basic syntax looks like this:
///
/// ```ignore
/// use reinda::{assets, Setup};
///
/// const ASSETS: Setup = assets! {
///     #![global_setting1 = "value1"]
///     #![global_setting2 = 3]
///
///     "path/to/asset-x.html": {
///         asset_setting_a: false,
///         asset_setting_b: "data",
///     },
///     "another-path/asset-y.js": {
///         asset_setting_b: "other data",
///     },
/// };
/// ```
///
/// ## Global settings
///
/// - **`base_path`** (string, required): specifies a base path. It is relative to
///   `CARGO_MANIFEST_DIR`. The resulting compile time path of an asset is
///   `$CARGO_MANIFEST_DIR/$base_path/$asset_path`. When assets are loaded at
///   runtime, everything is relative to the current directory instead of
///   `CARGO_MANIFEST_DIR`. You can overwrite the base path for the runtime via
///   [`Config::base_path`].
///
/// ## Assets
///
/// Each asset is defined by a path, followed by colon and then a set of
/// settings for that asset. In many cases, the path can simply be a file name.
/// See the `base_path` global setting.
///
/// The settings are specified with the Rust struct initializer syntax. However,
/// you can just omit fields for which you want to use the default value. Also,
/// boolean fields can omit their value if it is `true`. For example,
/// `"bundle.js": { template }` is the same as `"bundle.js": { template: true
/// }`.
///
///
/// Asset settings:
///
/// - **`serve`** (bool, default: `true`): if set to `false`, this asset cannot
///   be directly retrieved via [`Assets::get`]. This only makes sense for
///   assets that are intended to be included by another asset.
///
/// - **`template`** (bool, default: `false`): if set to `true`, the included
///   file is treated as a template with the `reinda` specific template syntax.
///   Otherwise it is treated as verbatim file.
///
/// - **`dynamic`** (bool, default: `false`): if set to `true`, this is treated
///   as a dynamic asset which has to be loaded at runtime and cannot be
///   embedded. In dev mode, the asset is loaded on each [`Assets::get`] (like
///   all other assets); in prod mode, it is loaded from the file system in
///   `Assets::new`.
///
/// - **`hash`** (optional pair of strings): see section about hashed filenames
///   below.
///
/// - **`prepend`/`append`** (optional string, default: `None`): if specified, a
///   fixed string is prepended or appended to the included asset before any
///   other processing (e.g. template) takes place.
///
///
/// ### Hashed filename
///
/// If `hash` is specified for an asset, a hash of the asset's contents are
/// included intoits filename.[`Assets::get`] won't serve it with the path you
/// specified in this macro, but with a path that includes a hash. Filename
/// hashing is disable in dev mode.
///
/// By default, the hash (and an additional `.`) will be inserted before the
/// first `.` in the filename of the asset's path. If that filename does not
/// contain a `.`, a `-` and the hash is appeneded to the filename. For
/// example:
///
/// - `sub/main.js.map` → `sub/main.JdeK1YeQ90aJ.js.map`
/// - `folder/raw-data` → `folder/raw-data-JdeK1YeQ90aJ`
///
/// If that doesn't suit you, you can override this behavior by specifying two
/// strings in between which the hash will be inserted. For example:
///
/// ```text
/// "main-v1.0-min.js.map": {
///     hash: "main-v1.0-min." ... ".js.map",
/// }
/// ```
///
/// The resulting filename would be `main-v1.0-min.JdeK1YeQ90aJ.js.map` for
/// example.
///
pub use reinda_macros::assets;

/// An opaque structure that holds metadata and (in prod mode) the included raw
/// asset data.
///
/// **Note**: the fields of this struct are public in order for it to be
/// const-constructed in [`assets!`]. There are also a some public methods on
/// this type for a similar reason. However, the fields and methods are not
/// considered part of the public API of `reinda` and as such you shouldn't use
/// them as they might change in minor version updates. Treat this type as
/// opaque! (In case you were wondering, all those fields and methods have been
/// hidden in the docs).
pub use reinda_core::Setup;

// We don't really want to expose those, but we are forced to in order for
// `assets!` to work.
pub use reinda_core::{AssetDef, AssetId, PathToIdMap};


/// Runtime configuration.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// The base path from which all assets are loaded. *Default*: `None`.
    ///
    /// The per-asset paths you defined in the `asset!` invocation are prepended
    /// by this path. This path can be absolute or relative. If this is not
    /// defined, the base path set in `assets!` is used.
    pub base_path: Option<PathBuf>,

    /// Key-value map for template variables. *Default*: empty.
    ///
    /// These can be inserted into assets via `{{: var:foo :}}`.
    pub variables: HashMap<String, String>,
}

/// A set of assets.
///
/// This is one of the two main entry points of this library. You create an
/// instance of this type via [`Assets::new`] and then retrieve asset data via
/// [`Assets::get`]. [The macro `assets!`][assets!] is the other main entry
/// point of this library. It generates a `Setup` value at compile time which
/// you have to pass to `Assets::new`.
#[derive(Debug)]
pub struct Assets {
    setup: Setup,
    config: Config,

    /// Stores the hashed paths of assets. This contains entries for hashed
    /// paths only; assets without `hash` are not present here.
    #[cfg(not(debug_assertions))]
    public_paths: AHashMap<AssetId, String>,

    /// Stores the actual asset data. The key is the public path. So this is
    /// basically the whole implementation of `Assets::get` in prod mode.
    #[cfg(not(debug_assertions))]
    assets: AHashMap<Box<str>, Bytes>,
}


impl Assets {
    /// Creates a new instance of this type and, in prod mode, prepares all
    /// assets.
    pub async fn new(setup: Setup, config: Config) -> Result<Self, Error> {
        Self::new_impl(setup, config).await
    }

    /// Returns the file contents of the asset referred to by `public_path`.
    ///
    /// The given path is the "public" path, as it is a part of the actual
    /// request URI. This doesn't mean this parameter has to be the full path
    /// component from the request URI; you likely want to serve your assets in
    /// a subdirectory, like `/assets/` which you would have to remove from the
    /// URI-path before calling this method. However, for assets with hashed
    /// filenames, this method expects the hashed path and not the one you
    /// specified in [`assets!`].
    ///
    /// If no asset with the specified path exists, `Ok(None)` is returned. An
    /// error is returned in debug mode for a variety of reasons. In release
    /// mode, this method always returns `Ok(_)`. See [`GetError`].
    pub async fn get(&self, public_path: &str) -> Result<Option<Bytes>, GetError> {
        #[cfg(debug_assertions)]
        {
            self.load_from_fs(public_path).await
        }

        #[cfg(not(debug_assertions))]
        {
            Ok(self.assets.get(public_path).cloned())
        }
    }

    /// Returns an iterator over the IDs of all assets.
    pub fn asset_ids(&self) -> impl Iterator<Item = AssetId> {
        (0..self.setup.assets.len()).map(|i| AssetId(i as u32))
    }

    /// Returns meta information about a specific asset, or `None` if no asset
    /// with the given ID exists.
    pub fn asset_info(&self, id: AssetId) -> Option<Info<'_>> {
        let def = self.setup.assets.get(id.0 as usize)?;
        let public_path = {
            #[cfg(not(debug_assertions))]
            {
                self.public_paths.get(&id).map(|s| &**s)
            }

            #[cfg(debug_assertions)]
            {
                None
            }
        };

        Some(Info {
            original_path: def.path,
            public_path,
            serve: def.serve,
            dynamic: def.dynamic,
        })
    }
}

// Private functions & methods.
impl Assets {
    /// Implementation of `new` for dev builds.
    #[cfg(debug_assertions)]
    async fn new_impl(setup: Setup, config: Config) -> Result<Self, Error> {
        Ok(Self { setup, config })
    }

    /// Implementation of `new` for prod builds.
    #[cfg(not(debug_assertions))]
    async fn new_impl(setup: Setup, config: Config) -> Result<Self, Error> {
        use crate::resolve::ResolveResult;

        let resolver = Resolver::for_all_assets(&setup, &config).await?;
        let ResolveResult { assets, public_paths } = resolver.resolve(&setup, &config)?;

        let assets = assets.into_iter()
            .filter(|(id, _)| setup.def(*id).serve)
            .map(|(id, bytes)| {
                let public_path = public_paths.get(&id)
                    .map(|s| &**s)
                    .unwrap_or(setup.def(id).path)
                    .into();
                (public_path, bytes)
            })
            .collect();

        Ok(Self {
            setup,
            config,
            public_paths,
            assets,
        })
    }

    /// Loads an asset from filesystem, dynamically resolving all includes and
    /// paths. This is the dev-build implementation of `Assets::get`.
    #[cfg(debug_assertions)]
    async fn load_from_fs(&self, start_path: &str) -> Result<Option<Bytes>, Error> {
        let id = match self.setup.path_to_id(start_path) {
            None => return Ok(None),
            Some(id) => id,
        };

        let setup = &self.setup;
        let config = &self.config;
        if !self.setup.def(id).serve {
            return Ok(None);
        }

        let resolver = Resolver::for_single_asset_from_fs(id, setup, config).await?;
        let resolved = resolver.resolve(setup, config)?;
        let out = {resolved.assets}.remove(&id)
            .expect("resolver did not contain requested file");

        Ok(Some(out))
    }
}


/// Error type for [`Assets::get`], which is different for dev and prod builds.
///
/// In dev mode, all required files are loaded from file system when you call
/// `Assets::get`. This can lead to errors (e.g. IO errors), so `Assets::get`
/// returns a `Result<_, Error>`. As such, in dev mode, `GetError` is just an
/// alias for [`Error`].
///
/// In prod mode however, all files are loaded and prepared in [`Assets::new`].
/// The `Assets::get` method will never produce an error. Therefore, in prod
/// mode, `GetError` is an alias to the never type, signaling that an error will
/// never happen.
#[cfg(debug_assertions)]
pub type GetError = Error;

/// See above.
#[cfg(not(debug_assertions))]
pub type GetError = std::convert::Infallible;

/// All errors that might be returned by `reinda`.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("IO error while accessing '{path}'")]
    Io {
        err: std::io::Error,
        path: PathBuf,
    },

    #[error("template error in '{file}': {err}")]
    Template {
        err: template::Error,
        file: String,
    },

    #[error("cyclic include detected: {0:?}")]
    CyclicInclude(Vec<String>),

    #[error("unresolved include in '{in_file}': asset '{included}' does not exist")]
    UnresolvedInclude {
        in_file: String,
        included: String,
    },

    #[error("invalid path reference `{{{{: path:{referenced} :}}}}` in '{in_file}': \
        referenced asset does not exist")]
    UnresolvedPath {
        in_file: String,
        referenced: String,
    },

    #[error("variable '{key}' is used in '{file}', but that variable has not been defined")]
    MissingVariable {
        key: String,
        file: String,
    },
}

/// Contains meta information about an asset.
#[derive(Debug)]
pub struct Info<'a> {
    original_path: &'static str,
    public_path: Option<&'a str>,
    serve: bool,
    dynamic: bool,
}

impl<'a> Info<'a> {
    /// Returns the original path specified in the [`assets!`] invocation.
    pub fn original_path(&self) -> &'static str {
        self.original_path
    }

    /// Returns the public path, which might be the same as `original_path` or
    /// might contain a hash if `hash` was specified in [`assets!`] for this
    /// asset.
    pub fn public_path(&self) -> &'a str {
        self.public_path.unwrap_or(self.original_path)
    }

    /// Returns whether or not this asset is publicly served. Equals the `serve`
    /// specification in the [`assets!`] macro.
    pub fn is_served(&self) -> bool {
        self.serve
    }

    /// Returns whether this asset is always loaded at runtime (either at
    /// startup or when requested) as opposed to being embeded. Equals the
    /// `dynamic` specification in the [`assets!`] macro.
    pub fn is_dynamic(&self) -> bool {
        self.dynamic
    }
}
