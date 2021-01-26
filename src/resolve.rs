use ahash::AHashMap;
use bytes::Bytes;
use std::path::Path;
use tokio::io::AsyncReadExt;

use reinda_core::{
    AssetDef, AssetId,
    template::{Fragment, Template},
};
use crate::{
    Config, Error, Setup,
    hash,
    dep_graph::DepGraph,
};


/// State to resolve multiple asset templates with includes and other
/// dependencies.
#[derive(Debug)]
pub(crate) struct Resolver {
    resolved: AHashMap<AssetId, Bytes>,
    unresolved: AHashMap<AssetId, Template>,
    graph: DepGraph,
}

impl Resolver {
    pub(crate) fn new() -> Self {
        Self {
            resolved: AHashMap::new(),
            unresolved: AHashMap::new(),
            graph: DepGraph::new(),
        }
    }

    /// Prepares a resolver to resolve all assets. The returned resolver
    /// satisfies the preconditions for `Self::resolve`.
    #[cfg(any(not(debug_assertions), feature = "debug-is-prod"))]
    pub(crate) async fn for_all_assets(setup: &Setup, config: &Config) -> Result<Resolver, Error> {
        let mut resolver = Resolver::new();
        for (id, asset_def) in setup.assets.iter().enumerate() {
            let id = AssetId(id as u32);

            // Explicitly add asset to graph, regardless on whether it has
            // dependencies or depends on anything else.
            resolver.graph.add_asset(id);

            let raw_bytes = if asset_def.dynamic {
                load_raw_from_fs(id, setup, config).await?
            } else {
                load_from_static(asset_def.content)
            };

            resolver.add_raw(raw_bytes, id, setup)?;
        }

        Ok(resolver)
    }

    /// Prepares a resolver to resolve a single asset. All files are loaded from
    /// the file system. The returned resolver satisfies the preconditions for
    /// `Self::resolve`.
    #[cfg(all(debug_assertions, not(feature = "debug-is-prod")))]
    pub(crate) async fn for_single_asset_from_fs(
        asset_id: AssetId,
        setup: &Setup,
        config: &Config,
    ) -> Result<Resolver, Error> {
        // Create new resolver and already register the requested file in the
        // graph.
        let mut resolver = Resolver::new();
        resolver.graph.add_asset(asset_id);

        // Load the raw content of the requested files and all files recursively
        // included by it.
        let mut stack = vec![asset_id];
        while let Some(id) = stack.pop() {
            // If we already loaded this file, skip it. Asset IDs might be added
            // to the stack multiple times if they are included by different
            // files.
            if resolver.resolved.contains_key(&id) || resolver.unresolved.contains_key(&id) {
                continue;
            }

            // Load file contents and add it to the resolver
            let raw_bytes = load_raw_from_fs(id, setup, config).await?;
            resolver.add_raw(raw_bytes, id, &setup)?;

            // Add all included assets to the stack to recursively check.
            stack.extend(resolver.graph.dependencies_of(id));
        }

        Ok(resolver)
    }

    /// Resolves all assets in `resolver`.
    ///
    /// Assumes that all (recursive) includes of all unresolved files in
    /// `resolver` are already loaded into the resolver, otherwise this method
    /// will panic! This implies that all include paths are valid and refer to
    /// assets in `self.setup`.
    pub(crate) fn resolve<'a>(
        self,
        setup: &Setup,
        config: &Config,
    ) -> Result<ResolveResult, Error> {
        let Resolver { mut resolved, mut unresolved, graph } = self;
        let mut public_paths: AHashMap<AssetId, String> = AHashMap::new();

        // We sort the include graph topologically such that we never have to
        // deal with an unresolved include.
        let assets = graph.topological_sort().map_err(|cycle| {
            let cycle = cycle.into_iter().map(|id| setup.def(id).path.to_string()).collect();
            Error::CyclicInclude(cycle)
        })?;

        for id in assets {
            let def = setup.def(id);

            // If `id` is not in `unresolved`, it is already resolved and we can
            // skip resolving it.
            if let Some(template) = unresolved.remove(&id) {
                let path = def.path;
                let rendered = template.render(|fragment, mut appender| -> Result<_, Error> {
                    match fragment {
                        Fragment::Include(p) => {
                            // Regarding `unwrap` and `expect`: see method
                            // preconditions.
                            let id = setup.path_to_id(&p).unwrap();
                            let data = resolved.get(&id)
                                .expect("missing include in `Assets::resolve`");

                            appender.append(&data);
                        }
                        Fragment::Path(p) => {
                            let reference_id = setup.path_to_id(&p)
                                .expect("missing path reference in `Assets::resolve`");
                            let public_path = public_paths.get(&reference_id)
                                .map(|s| &**s)
                                .unwrap_or(setup.def(reference_id).path);

                            appender.append(public_path.as_bytes());
                        }
                        Fragment::Var(key) => {
                            let value = config.variables.get(&key)
                                .ok_or_else(|| Error::MissingVariable {
                                    key: key.into(),
                                    file: path.into(),
                                })?;
                            appender.append(value.as_bytes());
                        }
                    }

                    Ok(())
                })?;

                resolved.insert(id, rendered);
            }

            // If a hashed filename is requested, calculate that filename so
            // that subsequent assets can use it.
            if def.hashed_filename() {
                let hashed = hash::hashed_path_of(def, &resolved[&id]);
                public_paths.insert(id, hashed);
            }
        }

        Ok(ResolveResult {
            assets: resolved,
            public_paths,
        })
    }

    fn add_raw(
        &mut self,
        raw_bytes: Bytes,
        asset_id: AssetId,
        setup: &Setup,
    ) -> Result<(), Error> {
        let raw_asset = RawAsset::new(raw_bytes, &setup.def(asset_id))?;
        match raw_asset.into_already_rendered() {
            Ok(resolved) => {
                // If there are no unresolved fragments at all, this is already
                // ready.
                self.resolved.insert(asset_id, resolved);
            }
            Err(template) => {
                // Go through all fragments to find dependencies of this asset
                // and add those to the graph.
                for fragment in template.fragments() {
                    let dep = dependency_in_fragment(fragment, asset_id, setup)?;
                    if let Some(dep) = dep {
                        self.graph.add_dependency(asset_id, dep);
                    }
                }

                self.unresolved.insert(asset_id, template);
            }
        }

        Ok(())
    }
}

pub(crate) struct ResolveResult {
    pub(crate) assets: AHashMap<AssetId, Bytes>,

    #[cfg_attr(debug_assertions, allow(dead_code))]
    pub(crate) public_paths: AHashMap<AssetId, String>,
}

/// Checks if the given fragment contains a dependency to another asset. If so,
/// returns `Some(_)`, `None` otherwise.
fn dependency_in_fragment(
    fragment: &Fragment,
    parent: AssetId,
    setup: &Setup,
) -> Result<Option<AssetId>, Error> {
    match fragment {
        Fragment::Include(p) => {
            setup.path_to_id(&p)
                .ok_or_else(|| Error::UnresolvedInclude {
                    in_file: setup.def(parent).path.into(),
                    included: p.into(),
                })
                .map(Some)
        }

        Fragment::Path(p) => {
            let referee_id = setup.path_to_id(p)
                .ok_or_else(|| Error::UnresolvedPath {
                    in_file: setup.def(parent).path.into(),
                    referenced: p.into(),
                })?;

            // If the referenced asset has a hashed file, this is considered a
            // dependency of `parent`. We first have to load `referee` to
            // caculate its hash to insert the correct path into `parent`.
            if setup.def(referee_id).hashed_filename() {
                Ok(Some(referee_id))
            } else {
                Ok(None)
            }
        }

        _ => Ok(None),
    }
}

/// Loads an asset from the file system and returns the raw bytes. IO errors
/// are returned. Caller should make sure that `path` is actually a valid,
/// listed asset.
async fn load_raw_from_fs(
    id: AssetId,
    setup: &Setup,
    config: &Config,
) -> Result<Bytes, Error> {
    let def = setup.def(id);
    let path =  match config.path_overrides.get(def.path) {
        Some(p) => p.clone(),
        None => {
            config.base_path.as_deref()
                .unwrap_or(Path::new(setup.base_path))
                .join(def.path)
        }
    };

    let mut out = Vec::new();
    if let Some(prepend) = def.prepend {
        out.extend_from_slice(prepend);
    }

    let mut file = tokio::fs::File::open(&path).await
        .map_err(|err| Error::Io { err, path: path.clone() })?;
    file.read_to_end(&mut out).await
        .map_err(|err| Error::Io { err, path: path.clone() })?;

    if let Some(append) = def.append {
        out.extend_from_slice(append);
    }

    Ok(Bytes::from(out))
}

#[cfg(any(not(debug_assertions), feature = "debug-is-prod"))]
fn load_from_static(raw: &'static [u8]) -> Bytes {
    #[cfg(feature = "compress")]
    {
        use flate2::bufread::DeflateDecoder;
        use std::io::Read;

        let mut decompressed = Vec::new();
        let mut decoder = DeflateDecoder::new(raw);
        decoder.read_to_end(&mut decompressed).expect("failed to decompress static data");
        Bytes::from(decompressed)
    }

    #[cfg(not(feature = "compress"))]
    Bytes::from_static(raw)
}

#[derive(Debug)]
enum RawAsset {
    /// A template asset that still has to be rendered.
    Template(Template),
    /// A literal asset that does not need any template processing.
    Literal(Bytes),
}

impl RawAsset {
    fn new(raw: Bytes, def: &AssetDef) -> Result<Self, Error> {
        if def.template {
            let t = Template::parse(raw)
                .map_err(|err| Error::Template { err, file: def.path.into() })?;
            Ok(Self::Template(t))
        } else {
            Ok(Self::Literal(raw))
        }
    }

    fn into_already_rendered(self) -> Result<Bytes, Template> {
        match self {
            Self::Template(t) => t.into_already_rendered(),
            Self::Literal(b) => Ok(b),
        }
    }
}
