use std::{io, marker::PhantomData, path::{Path, PathBuf}, sync::Arc};

use ahash::{HashMap, HashMapExt};
use bytes::Bytes;

use crate::{
    builder::EntryBuilderKind,
    Asset, BuildError, Builder, DataSource, Modifier, ModifierContext, SplitGlob,
};


pub(crate) struct AssetsInner(Arc<AssetsEvenMoreInner>);

pub(crate) struct AssetsEvenMoreInner {
    /// All specified assets, but not yet loaded.
    assets: HashMap<String, (DataSource, Modifier)>,

    /// List of glob patterns that were added. This is only relevant for the dev
    /// mode where we want to be able to load files dynamically in `get` that
    /// were not present during build or prepare-time.
    ///
    /// Sorted by the length of `http_prefix`, starting with the longest.
    globs: Vec<DevGlobEntry>,
}

struct DevGlobEntry {
    http_prefix: String,
    glob: SplitGlob,
    modifier: Modifier,
    base_path: &'static Path,
}

impl AssetsInner {
    pub(crate) async fn build(builder: Builder<'_>) -> Result<Self, BuildError> {
        // Collect all glob entries we have.
        let globs = builder.assets.iter().filter_map(|ab| {
            if let EntryBuilderKind::Glob { http_prefix, glob, base_path, .. } = &ab.kind {
                Some(DevGlobEntry {
                    http_prefix: (*http_prefix).to_owned(),
                    glob: glob.clone(),
                    modifier: ab.modifier.clone(),
                    base_path: Path::new(*base_path),
                })
            } else {
                None
            }
        }).collect();

        // Collect all files we know about.
        let mut assets = HashMap::with_capacity(builder.assets.len());
        for ab in builder.assets {
            match ab.kind {
                EntryBuilderKind::Single { http_path, source } => {
                    assets.insert(http_path.to_owned(), (source, ab.modifier));
                }
                EntryBuilderKind::Glob { http_prefix, files, .. } => {
                    for file in files {
                        assets.insert(
                            format!("{http_prefix}/{}", file.suffix),
                            (file.source, ab.modifier.clone()),
                        );
                    }
                }
            }
        }

        Ok(Self(Arc::new(AssetsEvenMoreInner { assets, globs })))
    }

    pub(crate) fn get(&self, http_path: &str) -> Option<Asset> {
        self.0.assets.get(http_path)
            .cloned()
            // In dev mode, we also check if the requested file matches a glob
            // and if so, we check the file system.
            .or_else(|| {
                self.0.match_globs(http_path)
                    .filter(|(path, _)| path.exists())
                    .map(|(path, modifier)| (DataSource::File(path), modifier))
            })
            .map(|(source, modifier)| Asset(AssetInner {
                source,
                modifier,
                assets: self.0.clone(),
            }))
    }
}

impl AssetsEvenMoreInner {
    fn match_globs(&self, http_path: &str) -> Option<(PathBuf, Modifier)> {
        self.globs.iter().find_map(|item| {
            http_path.strip_prefix(&item.http_prefix)
                .filter(|suffix| item.glob.suffix.matches(suffix))
                .map(|suffix| (
                    item.base_path.join(item.glob.prefix).join(suffix),
                    item.modifier.clone(),
                ))
        })
    }
}


/// An asset.
///
/// Very cheap to clone (in prod mode anyway, which is the only thing that
/// matters).
#[derive(Clone)]
pub(crate) struct AssetInner {
    source: DataSource,
    modifier: Modifier,
    assets: Arc<AssetsEvenMoreInner>,
}

impl AssetInner {
    /// Returns the contents of this asset. Will be loaded from the file system
    /// in dev mode, potentially returning IO errors. In prod mode, the file
    /// contents are already loaded and this method always returns `Ok(_)`.
    pub(crate) async fn content(&self) -> Result<Bytes, io::Error> {
        let bytes = self.source.load().await.map_err(|(e, _)| e)?;

        // Apply modifications, if specified.
        let modified =  match &self.modifier {
            Modifier::None => bytes,

            // // Since in dev mode, hashed paths are not used, no
            // // modifications are necessary.
            // Modifier::AutoPathReplacer => bytes,

            // The `PathMap::empty()` might allocate but we are in dev mode,
            // we don't care.
            Modifier::Custom { f, deps } => f(bytes, ModifierContext {
                declared_deps: &deps,
                inner: ModifierContextInner {
                    assets: self.assets.clone(),
                    _dummy: PhantomData,
                },
            }),
        };

        Ok(modified)
    }

    pub(crate) fn is_filename_hashed(&self) -> bool {
        false
    }
}


pub(crate) struct ModifierContextInner<'a> {
    assets: Arc<AssetsEvenMoreInner>,
    _dummy: PhantomData<&'a ()>,
}

impl<'a> ModifierContextInner<'a> {
    pub(crate) fn resolve_path<'b>(&'b self, path: &'b str) -> Option<&'b str> {
        if self.assets.assets.contains_key(path) || self.assets.match_globs(path).is_some() {
            Some(path)
        } else {
            None
        }
    }
}
