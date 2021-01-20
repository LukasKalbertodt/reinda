use std::{collections::HashMap, path::PathBuf};
use bytes::Bytes;

#[cfg(not(debug_assertions))]
use ahash::AHashMap;

use reinda_core::template;
use crate::resolve::Resolver;

mod include_graph;
mod resolve;



pub use reinda_macros::assets;
pub use reinda_core::{AssetDef, AssetId, PathToIdMap, Setup};

/// Runtime assets configuration.
#[derive(Debug, Clone, Default)]
pub struct Config {
    pub base_path: Option<PathBuf>,
    pub variables: HashMap<String, String>,
    // compression
}

#[derive(Debug)]
pub struct Assets {
    setup: Setup,

    // TODO: maybe wrap into `RwLock` to make changable later.
    config: Config,

    /// Stores the hashed paths of assets. This contains entries for hashed
    /// paths only; assets without `hash` are not present here.
    #[cfg(not(debug_assertions))]
    hashed_paths: AHashMap<AssetId, String>,

    #[cfg(not(debug_assertions))]
    assets: AHashMap<Box<str>, Bytes>,
}


impl Assets {
    pub async fn new(setup: Setup, config: Config) -> Result<Self, Error> {
        Self::new_impl(setup, config).await
    }

    /// Returns the file contents of the asset referred to by `public_path`.
    ///
    /// The given path is the "public" path, as it is a part of the actual
    /// request URI. This doesn't mean this parameter has to be the full path
    /// component from the request URI; you likely want to serve your assets
    /// under a subdirectory, like `/assets/` which you would have to remove
    /// from the URI-path before calling this method. However, for assets with
    /// hashed filenames, this method expects the hashed path and not the one
    /// you specified in `assets!`.
    ///
    /// If no asset with the specified path exists, `Ok(None)` is returned. An
    /// error is returned in debug mode for a variety of reasons. In release
    /// mode, this method always returns `Ok(_)`. TODO: change return type to
    /// typedef error never.
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

    /// Returns the public path of the specified asset, i.e. the path that one
    /// would pass to `get`. TODO
    ///
    /// If the specified asset has a hashed path, that is returned. Otherwise
    /// this function returns the same string as specified when defining the
    /// asset with `assets!`.
    pub fn public_path_of(&self, id: AssetId) -> &str {
        #[cfg(not(debug_assertions))]
        {
            if let Some(s) = self.hashed_paths.get(&id) {
                return s;
            }
        }

        // If this point is reached, either the asset's path is not hashed or we
        // are in debug mode, where we never hash paths.
        self.setup[id].path
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
        let resolver = Resolver::for_all_assets(&setup, &config).await?;
        // TODO: hashing
        let resolved = resolver.resolve(&setup, &config, |id| setup[id].path)?;
        let assets = resolved.into_iter()
            .map(|(id, bytes)| (setup[id].path.into(), bytes))
            .collect();

        Ok(Self {
            setup,
            config,
            hashed_paths: AHashMap::new(), // TODO: hashing
            assets,
        })
    }

    /// Loads an asset from filesystem, dynamically resolving all includes and
    /// paths. This is the dev-build implementation of `Assets::get`.
    #[cfg(debug_assertions)]
    async fn load_from_fs(&self, start_path: &str) -> Result<Option<Bytes>, Error> {
        let start_id = match self.setup.path_to_id(start_path) {
            None => return Ok(None),
            Some(id) => id,
        };

        let setup = &self.setup;
        let config = &self.config;
        let resolver = Resolver::for_single_asset_from_fs(start_id, setup, config).await?;
        let resolved = resolver.resolve(setup, config, |id| setup[id].path)?;
        let out = {resolved}.remove(&start_id)
            .expect("resolver did not contain requested file");

        Ok(Some(out))
    }
}


/// Error type for [`Assets::get`], which is different for dev and prod builds.
///
/// In dev mode, all files are loaded from file system when you call `get`. This
/// can lead to errors (i.e. IO errors), so `get` returns a `Result<_, Error>`.
/// As such, in dev mode, this is just an alias for [`Error`].
///
/// In prod mode however, all files are loaded and prepared in [`Assets::new`].
/// The `get` method will never produce an error. Therefore, in prod mode, this
/// is an alias to the never type, signaling that an error will never happen.
#[cfg(debug_assertions)]
pub type GetError = Error;

/// See above.
#[cfg(not(debug_assertions))]
pub type GetError = std::convert::Infallible;

/// All errors that might be returned by `reinda`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error")]
    Io(#[from] std::io::Error),

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
