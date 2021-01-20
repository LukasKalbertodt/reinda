use std::{borrow::Cow, collections::HashMap, path::PathBuf};
use bytes::Bytes;
use ahash::AHashMap;

use reinda_core::template;
use crate::include_graph::IncludeGraph;

mod include_graph;



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
}


impl Assets {
    pub async fn new(setup: Setup, config: Config) -> Result<Self, Error> {
        Ok(Self {
            setup,
            config,

            #[cfg(not(debug_assertions))]
            hashed_paths: AHashMap::new(),
        })
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
    pub async fn get(&self, public_path: &str) -> Result<Option<Bytes>, Error> {
        self.load_dynamic(public_path).await
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

    /// Loads an asset but does not attempt to render it as a template. Thus,
    /// the returned data might not be ready to be served yet.
    async fn load_raw(&self, path: &str) -> Result<RawAsset, Error> {
        let content = {
            #[cfg(debug_assertions)]
            {
                use std::path::Path;

                let base = self.config.base_path.as_deref()
                    .unwrap_or(Path::new(self.setup.base_path));

                Bytes::from(tokio::fs::read(base.join(path)).await?)
            }

            #[cfg(not(debug_assertions))]
            {
                let asset = self.setup.asset_by_path(path)
                    .expect("called `read_raw` with invalid path");
                Bytes::from_static(asset.content)
            }
        };

        let mut unresolved_fragments = Vec::new();
        for span in template::FragmentSpans::new(&content) {
            unresolved_fragments.push(Fragment::from_bytes(&content[span], path)?);
        }

        Ok(RawAsset {
            content,
            unresolved_fragments,
        })
    }

    async fn load_dynamic(&self, start_path: &str) -> Result<Option<Bytes>, Error> {
        match self.setup.path_to_id(start_path) {
            None => Ok(None),
            Some(start_id) => {
                let resolver = self.dynamic_single_file_resolver(start_id).await?;
                let resolved = self.resolve(resolver)?;
                let out = {resolved}.remove(&start_id)
                    .expect("resolver did not contain requested file");

                Ok(Some(out))
            }
        }
    }

    /// Prepares a resolver to resolve a single file. The returned resolver
    /// satisfies the preconditions for `Self::resolve`.
    async fn dynamic_single_file_resolver(&self, asset_id: AssetId) -> Result<Resolver, Error> {
        // Load the raw content of the requested files and all files recursively
        // included by it.
        let mut resolver = Resolver::new();
        let mut stack = vec![asset_id];
        while let Some(id) = stack.pop() {
            // If we already loaded this file, skip it. Asset IDs might be added
            // to the stack multiple times if they are included by different
            // files.
            if resolver.is_loaded(id) {
                continue;
            }

            let path = self.setup[id].path;
            let raw = self.load_raw(path).await?;

            if raw.unresolved_fragments.is_empty() {
                // If there are no unresolved fragments at all, this is already
                // ready.
                resolver.resolved.insert(id, raw.content);
            } else {
                // If there are still some templat fragments in the file, we
                // need to resolve this later.
                resolver.unresolved.insert(id, raw.content);

                // Add all included assets to the stack to recursively check.
                let includes = raw.unresolved_fragments.into_iter().filter_map(|f| f.as_include());
                for include_path in includes {
                    let includee_id = self.setup.path_to_id(&include_path)
                        .ok_or_else(|| Error::UnresolvedInclude {
                            in_file: path.into(),
                            included: include_path.into(),
                        })?;

                    resolver.graph.add_include(id, includee_id);
                    stack.push(includee_id);
                }
            }
        }

        Ok(resolver)
    }

    /// Resolves all assets in `resolver`.
    ///
    /// Assumes that all (recursive) includes of all unresolved files in
    /// `resolver` are already loaded into the resolver, otherwise this method
    /// will panic! This implies that all include paths are valid and refer to
    /// assets in `self.setup`.
    fn resolve(&self, resolver: Resolver) -> Result<AHashMap<AssetId, Bytes>, Error> {
        let Resolver { mut resolved, mut unresolved, graph } = resolver;

        // We sort the include graph topologically such that we never have to
        // deal with an unresolved include.
        let assets = graph.topological_sort().map_err(|cycle| {
            let cycle = cycle.into_iter().map(|id| self.setup[id].path.to_string()).collect();
            Error::CyclicInclude(cycle)
        })?;

        for idx in assets {
            let template = match unresolved.remove(&idx) {
                // If `idx` is ont in `unresolved`, it is already resolved and
                // we can skip it.
                None => continue,
                Some(template) => template,
            };

            let path = self.setup[idx].path;
            let resolved_template = template::render(&template, |inner, mut appender| -> Result<_, Error> {
                match Fragment::from_bytes(inner, path)? {
                    Fragment::Path(p) => {
                        let id = self.setup.path_to_id(&p)
                            .ok_or_else(|| Error::UnresolvedPath {
                                in_file: path.into(),
                                referenced: p.into(),
                            })?;

                        appender.append(self.public_path_of(id).as_bytes());
                    }
                    Fragment::Include(path) => {
                        // Regarding `unwrap` and `expect`: see method
                        // preconditions.
                        let id = self.setup.path_to_id(&path).unwrap();
                        let data = resolved.get(&id)
                            .expect("missing include in `Assets::resolve`");

                        appender.append(&data);
                    }
                    Fragment::Var(key) => {
                        let value = self.config.variables.get(&key)
                            .ok_or_else(|| Error::MissingVariable {
                                key: key.into(),
                                file: path.into(),
                            })?;
                        appender.append(value.as_bytes());
                    }
                }

                Ok(())
            })?;

            let bytes = match resolved_template {
                Cow::Borrowed(_) => template.clone(),
                Cow::Owned(v) => Bytes::from(v),
            };
            resolved.insert(idx, bytes);
        }

        Ok(resolved)
    }
}

/// All errors that might be returned by `reinda`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("template fragment does not contain valid UTF8: {0:?}")]
    NonUtf8TemplateFragment(Vec<u8>),

    #[error("unknown template fragment specifier '{specifier}' in file '{path}'")]
    UnknownTemplateSpecifier {
        specifier: String,
        path: String,
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

/// An asset that has been loaded but which might still need to be rendered as
/// template.
#[derive(Debug)]
pub struct RawAsset {
    pub content: Bytes,
    pub unresolved_fragments: Vec<Fragment>,
}

/// A parsed fragment in the template.
#[derive(Debug)]
pub enum Fragment {
    Path(String),
    Include(String),
    Var(String),
}

impl Fragment {
    fn from_bytes(bytes: &[u8], path: &str) -> Result<Self, Error> {
        let val = |s: &str| s[s.find(':').unwrap() + 1..].to_string();

        let s = std::str::from_utf8(bytes)
            .map_err(|_| Error::NonUtf8TemplateFragment(bytes.into()))?
            .trim();

        match () {
            () if s.starts_with("path:") => Ok(Self::Path(val(s))),
            () if s.starts_with("include:") => Ok(Self::Include(val(s))),
            () if s.starts_with("var:") => Ok(Self::Var(val(s))),

            _ => {
                let specifier = s[..s.find(':').unwrap_or(s.len())].to_string();
                Err(Error::UnknownTemplateSpecifier {
                    specifier,
                    path: path.to_string(),
                })
            }
        }
    }

    fn as_include(self) -> Option<String> {
        match self {
            Self::Include(p) => Some(p),
            _ => None,
        }
    }
}



struct Resolver {
    resolved: AHashMap<AssetId, Bytes>,
    unresolved: AHashMap<AssetId, Bytes>,
    graph: IncludeGraph,
}

impl Resolver {
    fn new() -> Self {
        Self {
            resolved: AHashMap::new(),
            unresolved: AHashMap::new(),
            graph: IncludeGraph::new(),
        }
    }

    fn is_loaded(&self, id: AssetId) -> bool {
        self.resolved.contains_key(&id) || self.unresolved.contains_key(&id)
    }
}
