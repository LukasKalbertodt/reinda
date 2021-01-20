use std::{borrow::Cow, path::PathBuf};
use bytes::Bytes;
use ahash::AHashMap as HashMap;

use reinda_core::template;
use crate::include_graph::IncludeGraph;

mod include_graph;



pub use reinda_macros::assets;
pub use reinda_core::{AssetDef, AssetId, PathToIdMap, Setup};

/// Runtime assets configuration.
#[derive(Debug, Clone, Default)]
pub struct Config {
    base_path: Option<PathBuf>,
    variables: HashMap<String, String>,
    // compression
}

#[derive(Debug)]
pub struct Assets {
    setup: Setup,

    // TODO: maybe wrap into `RwLock` to make changable later.
    config: Config,
}


impl Assets {
    pub async fn new(setup: Setup, config: Config) -> Result<Self, Error> {
        Ok(Self {
            setup,
            config,
            // assets,
        })
    }

    /// Loads an asset but does not attempt to render it as a template. Thus,
    /// the returned data might not be ready to be served yet.
    pub async fn load_raw(&self, path: &str) -> Result<RawAsset, Error> {
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

    pub async fn load_dynamic(&self, start_path: &str) -> Result<Option<Bytes>, Error> {
        let start_idx = match self.setup.path_to_id(start_path) {
            None => return Ok(None),
            Some(idx) => idx,
        };

        // Step 1: Load the raw content of the requested files and all files
        // recursively included by it.
        let mut files = HashMap::new();
        let mut graph = IncludeGraph::new();
        let mut stack = vec![start_idx];
        while let Some(idx) = stack.pop() {
            // If we already loaded this file, skip it. Asset IDs might be added
            // to the stack multiple times if they are included multiple times.
            if files.contains_key(&idx) {
                continue;
            }

            let path = self.setup[idx].path;
            let raw = self.load_raw(path).await?;

            if raw.unresolved_fragments.is_empty() {
                // If there are no unresolved fragments at all, this is already
                // ready.
                files.insert(idx, ResolvingAsset::Resolved(raw.content));
            } else {
                // If there are still some templat fragments in the file, we
                // need to resolve this later.
                files.insert(idx, ResolvingAsset::Unresolved(raw.content));

                // Add all included assets to the stack to recursively check.
                let includes = raw.unresolved_fragments.into_iter().filter_map(|f| f.as_include());
                for import_path in includes {
                    let includee_idx = self.setup.path_to_id(&import_path)
                        .ok_or_else(|| Error::UnresolvedImport {
                            in_file: path.into(),
                            imported: import_path.into(),
                        })?;

                    graph.add_include(idx, includee_idx);
                    stack.push(includee_idx);
                }
            }
        }

        // Step 2: Now actually render the templates. We render in the
        // topological sort order such that we never have to deal with an
        // unresolved include.
        let assets = graph.topological_sort().map_err(|cycle| {
            let cycle = cycle.into_iter().map(|id| self.setup[id].path.to_string()).collect();
            Error::CyclicInclude(cycle)
        })?;

        for idx in assets {
            let template = match &files[&idx] {
                ResolvingAsset::Resolved(_) => continue,
                ResolvingAsset::Unresolved(template) => template,
            };

            let path = self.setup[idx].path;
            let resolved = template::render(&template, |inner, mut appender| -> Result<_, Error> {
                match Fragment::from_bytes(inner, path)? {
                    Fragment::Path(p) => appender.append(p.as_bytes()),
                    Fragment::Include(path) => {
                        let id = self.setup.path_to_id(&path).unwrap(); // already checked above
                        let data = files[&id].unwrap_resolved();
                        appender.append(&data);
                    }
                    Fragment::Var(_) => {
                        appender.append(b"TODO");
                    }
                }

                Ok(())
            })?;

            let bytes = match resolved {
                Cow::Borrowed(_) => template.clone(),
                Cow::Owned(v) => Bytes::from(v),
            };
            *files.get_mut(&idx).unwrap() = ResolvingAsset::Resolved(bytes);
        }

        Ok(Some(files[&start_idx].unwrap_resolved().clone()))
    }
}

/// All errors that might be returned by `reinda`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("template fragment does not contain valid UTF8: {0:?}")]
    NonUtf8TemplateFragment(Vec<u8>),

    #[error("unknown template fragment specifier {specifier} in file {path}")]
    UnknownTemplateSpecifier {
        specifier: String,
        path: String,
    },

    #[error("cyclic include detected: {0:?}")]
    CyclicInclude(Vec<String>),

    #[error("unresolved import in '{in_file}': asset '{imported}' does not exist")]
    UnresolvedImport {
        in_file: String,
        imported: String,
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
    state: HashMap<AssetId, ResolvingAsset>,
}

enum ResolvingAsset {
    Unresolved(Bytes),
    Resolved(Bytes),
}

impl ResolvingAsset {
    fn unwrap_resolved(&self) -> &Bytes {
        match self {
            Self::Unresolved(_) => panic!("called `unwrap_resolved` on an unresolved asset"),
            Self::Resolved(bytes) => bytes,
        }
    }
}

impl Resolver {
    fn new() -> Self {
        Self {
            state: HashMap::new(),
        }
    }
}
