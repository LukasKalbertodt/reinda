use std::io;

use ahash::{HashMap, HashMapExt};
use bytes::Bytes;

use crate::{
    builder::EntryBuilderKind, Asset, BuildError, Builder, DataSource, Modifier,
    ModifierContext, EntryBuilder, PathHash,
    dep_graph::DepGraph,
    hash::PathMap,
};


pub(crate) struct AssetsInner {
    assets: HashMap<String, Asset>,
}


#[derive(Clone)]
pub(crate) struct AssetInner {
    content: Bytes,
    hashed_filename: bool,
}

impl AssetsInner {
    pub(crate) async fn build(builder: Builder<'_>) -> Result<Self, BuildError> {
        // First we flatten our entries into a list of files to be loaded/resolved.
        let mut unresolved = HashMap::with_capacity(builder.assets.len());
        for EntryBuilder { kind, path_hash, modifier } in builder.assets {
            match kind {
                EntryBuilderKind::Single { http_path, source } => {
                    unresolved.insert(http_path.to_owned(), UnresolvedAsset {
                        source,
                        modifier,
                        path_hash,
                    });
                }
                EntryBuilderKind::Glob { http_prefix, files, .. } => {
                    for file in files {
                        let key = format!("{http_prefix}{}", file.suffix);
                        let value = UnresolvedAsset {
                            source: file.source,
                            modifier: modifier.clone(),
                            path_hash,
                        };
                        unresolved.insert(key, value);
                    }
                }
            };
        }

        // Next: build the dep graph.
        let mut dep_graph = DepGraph::new();
        for (unhashed_http_path, asset) in &unresolved {
            dep_graph.add_asset(&unhashed_http_path);
            if let Modifier::Custom { deps, .. } = &asset.modifier {
                for dep in deps {
                    dep_graph.add_dependency(&unhashed_http_path, &dep);
                }
            }
        }

        // Finally go over all assets, load and prepare them.
        let sorting = dep_graph.topological_sort().map_err(|cycle| {
            BuildError::CyclicDependencies(cycle.into_iter().map(|s| s.to_owned()).collect())
        })?;
        let mut assets = HashMap::new();
        let mut path_map = PathMap::new();
        for path in sorting {
            let asset = unresolved.get(path).unwrap();

            // Apply modifier
            let raw = asset.source.load().await
                .map_err(|(err, path)| BuildError::Io { err, path: path.to_owned() })?;
            let content = match &asset.modifier {
                Modifier::None => raw,
                Modifier::Custom { f, deps } => {
                    f(raw, ModifierContext {
                        declared_deps: &deps,
                        inner: ModifierContextInner {
                            path_map: &path_map,
                            unresolved: &unresolved,
                        },
                    })
                },
            };

            // Potentially hash filename
            let final_path = crate::hash::path_of(asset.path_hash, &path, &content, &mut path_map);

            assets.insert(final_path, Asset(AssetInner {
                content,
                hashed_filename: !matches!(asset.path_hash, PathHash::None),
            }));
        }

        Ok(Self { assets })
    }

    pub(crate) fn get(&self, http_path: &str) -> Option<Asset> {
        self.assets.get(http_path).cloned()
    }

    pub(crate) fn len(&self) -> usize {
        self.assets.len()
    }

    pub(crate) fn iter(&self) -> impl '_ + Iterator<Item = Asset> {
        self.assets.values().cloned()
    }
}

impl AssetInner {
    /// Returns the contents of this asset. Will be loaded from the file system
    /// in dev mode, potentially returning IO errors. In prod mode, the file
    /// contents are already loaded and this method always returns `Ok(_)`.
    pub(crate) async fn content(&self) -> Result<Bytes, io::Error> {
        Ok(self.content.clone())
    }

    pub(crate) fn is_filename_hashed(&self) -> bool {
        self.hashed_filename
    }
}


struct UnresolvedAsset<'a> {
    source: DataSource,
    modifier: Modifier,
    path_hash: PathHash<'a>,
}

pub(crate) struct ModifierContextInner<'a> {
    path_map: &'a PathMap<'a>,
    unresolved: &'a HashMap<String, UnresolvedAsset<'a>>,
}

impl<'a> ModifierContextInner<'a> {
    pub(crate) fn resolve_path<'b>(&'b self, unhashed_http_path: &'b str) -> Option<&'b str> {
        self.path_map.get(unhashed_http_path).or_else(|| {
            if self.unresolved.contains_key(unhashed_http_path) {
                Some(unhashed_http_path)
            } else {
                None
            }
        })
    }
}
