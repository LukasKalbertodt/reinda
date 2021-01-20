use std::path::Path;
use bytes::Bytes;
use ahash::AHashMap;

use reinda_core::{
    AssetDef, AssetId,
    template::{Fragment, Template},
};
use crate::{
    Config, Error, Setup,
    include_graph::IncludeGraph,
};


/// State to resolve multiple asset templates with includes and other
/// dependencies.
pub(crate) struct Resolver {
    resolved: AHashMap<AssetId, Bytes>,
    unresolved: AHashMap<AssetId, Template>,
    graph: IncludeGraph,
}

impl Resolver {
    pub(crate) fn new() -> Self {
        Self {
            resolved: AHashMap::new(),
            unresolved: AHashMap::new(),
            graph: IncludeGraph::new(),
        }
    }

    /// Prepares a resolver to resolve all assets. The returned resolver
    /// satisfies the preconditions for `Self::resolve`.
    #[cfg(not(debug_assertions))]
    pub(crate) async fn for_all_assets(setup: &Setup, config: &Config) -> Result<Resolver, Error> {
        let mut resolver = Resolver::new();
        for (id, asset_def) in setup.assets.iter().enumerate() {
            let id = AssetId(id as u32);

            let raw_bytes = if asset_def.dynamic {
                load_raw_from_fs(asset_def.path, setup, config).await?
            } else {
                Bytes::from_static(asset_def.content)
            };

            resolver.add_raw(raw_bytes, id, setup)?;
        }

        Ok(resolver)
    }

    /// Prepares a resolver to resolve a single asset. All files are loaded from
    /// the file system. The returned resolver satisfies the preconditions for
    /// `Self::resolve`.
    #[cfg(debug_assertions)]
    pub(crate) async fn for_single_asset_from_fs(
        asset_id: AssetId,
        setup: &Setup,
        config: &Config,
    ) -> Result<Resolver, Error> {
        // Load the raw content of the requested files and all files recursively
        // included by it.
        let mut resolver = Resolver::new();
        let mut stack = vec![asset_id];
        while let Some(id) = stack.pop() {
            // If we already loaded this file, skip it. Asset IDs might be added
            // to the stack multiple times if they are included by different
            // files.
            if resolver.resolved.contains_key(&id) || resolver.unresolved.contains_key(&id) {
                continue;
            }

            // Load file contents and add it to the resolver
            let path = setup[id].path;
            let raw_bytes = load_raw_from_fs(path, setup, config).await?;
            resolver.add_raw(raw_bytes, id, &setup)?;

            // Add all included assets to the stack to recursively check.
            stack.extend(resolver.graph.includes_of(id));
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
        mut public_path_of: impl FnMut(AssetId) -> &'a str,
    ) -> Result<AHashMap<AssetId, Bytes>, Error> {
        let Resolver { mut resolved, mut unresolved, graph } = self;

        // We sort the include graph topologically such that we never have to
        // deal with an unresolved include.
        let assets = graph.topological_sort().map_err(|cycle| {
            let cycle = cycle.into_iter().map(|id| setup[id].path.to_string()).collect();
            Error::CyclicInclude(cycle)
        })?;

        for idx in assets {
            let template = match unresolved.remove(&idx) {
                // If `idx` is ont in `unresolved`, it is already resolved and
                // we can skip it.
                None => continue,
                Some(template) => template,
            };

            let path = setup[idx].path;
            let rendered = template.render(|fragment, mut appender| -> Result<_, Error> {
                match fragment {
                    Fragment::Path(p) => {
                        let id = setup.path_to_id(&p)
                            .ok_or_else(|| Error::UnresolvedPath {
                                in_file: path.into(),
                                referenced: p.into(),
                            })?;

                        appender.append(public_path_of(id).as_bytes());
                    }
                    Fragment::Include(path) => {
                        // Regarding `unwrap` and `expect`: see method
                        // preconditions.
                        let id = setup.path_to_id(&path).unwrap();
                        let data = resolved.get(&id)
                            .expect("missing include in `Assets::resolve`");

                        appender.append(&data);
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

            resolved.insert(idx, rendered);
        }

        Ok(resolved)
    }

    fn add_raw(
        &mut self,
        raw_bytes: Bytes,
        asset_id: AssetId,
        setup: &Setup,
    ) -> Result<(), Error> {
        let raw_asset = RawAsset::new(raw_bytes, &setup[asset_id])?;
        match raw_asset.into_already_rendered() {
            Ok(resolved) => {
                // If there are no unresolved fragments at all, this is already
                // ready.
                self.resolved.insert(asset_id, resolved);
            }
            Err(template) => {
                // Go through all includes and add them to our graph.
                let includes = template.fragments().filter_map(|f| f.as_include());
                for include_path in includes {
                    let includee_id = setup.path_to_id(&include_path)
                        .ok_or_else(|| Error::UnresolvedInclude {
                            in_file: setup[asset_id].path.into(),
                            included: include_path.into(),
                        })?;

                    self.graph.add_include(asset_id, includee_id);
                }

                self.unresolved.insert(asset_id, template);
            }
        }

        Ok(())
    }
}

/// Loads an asset from the file system and returns the raw bytes. IO errors
/// are returned. Caller should make sure that `path` is actually a valid,
/// listed asset.
async fn load_raw_from_fs(path: &str, setup: &Setup, config: &Config) -> Result<Bytes, Error> {
    let base = config.base_path.as_deref()
        .unwrap_or(Path::new(setup.base_path));
    let content = tokio::fs::read(base.join(path)).await?;

    Ok(Bytes::from(content))
}

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
