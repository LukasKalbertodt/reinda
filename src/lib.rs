use std::{borrow::Cow, collections::HashMap, path::PathBuf};
use bytes::Bytes;

use reinda_core::template;
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
    pub async fn load_raw(&self, path: &str) -> Result<Option<RawAsset>, Error> {
        let content = {
            #[cfg(debug_assertions)]
            {
                use std::path::Path;

                let base = self.config.base_path.as_deref()
                    .unwrap_or(Path::new(self.setup.base_path));

                match tokio::fs::read(base.join(path)).await {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(e) => Err(e)?,
                    Ok(content) => Bytes::from(content),
                }
            }

            #[cfg(not(debug_assertions))]
            {
                match self.setup.asset_by_path(path) {
                    Some(asset) => Bytes::from_static(asset.content),
                    None => return Ok(None),
                }
            }
        };

        let mut unresolved_fragments = Vec::new();
        for span in template::FragmentSpans::new(&content) {
            unresolved_fragments.push(Fragment::from_bytes(&content[span], path)?);
        }

        Ok(Some(RawAsset {
            content,
            unresolved_fragments,
        }))
    }

    pub async fn load_single(&self, start_path: &str) -> Result<Option<Bytes>, Error> {
        let mut resolver = Resolver::new();

        let start_idx = match self.setup.path_to_id(start_path) {
            None => return Ok(None),
            Some(idx) => idx,
        };

        // Step 1: Load the raw content of the requested files and all files
        // recursively included by it.
        let mut queue = vec![start_idx];
        let mut queue_i = 0;
        while let Some(&idx) = queue.get(queue_i) {
            if resolver.state.contains_key(&idx) {
                return Err(Error::CyclicInclude(start_path.to_string()));
            }

            let path = self.setup[idx].path;
            let raw = match self.load_raw(path).await? {
                None => return Ok(None),
                Some(raw) => raw,
            };

            if raw.unresolved_fragments.is_empty() {
                resolver.state.insert(idx, ResolvingAsset::Resolved(raw.content));
            } else {
                for fragment in raw.unresolved_fragments {
                    match fragment {
                        Fragment::Include(import_path) => {
                            if let Some(idx) = self.setup.path_to_id(&import_path) {
                                queue.push(idx);
                            } else {
                                return Err(Error::UnresolvedImport {
                                    in_file: path.into(),
                                    imported: import_path. into(),
                                });
                            }
                        }
                        _ => {} // TODO: hashing
                    }
                }

                resolver.state.insert(idx, ResolvingAsset::Unresolved(raw.content));
            }

            queue_i += 1;
        }

        // Step 2: Now iterate the queue backwards and actually render the
        // templates. All the necessary data is already loaded now.
        for &idx in queue.iter().rev() {
            let template = match &resolver.state[&idx] {
                ResolvingAsset::Resolved(_) => continue,
                ResolvingAsset::Unresolved(template) => template,
            };

            let path = self.setup[idx].path;
            let resolved = template::render(&template, |inner, mut appender| -> Result<_, Error> {
                match Fragment::from_bytes(inner, path)? {
                    Fragment::Path(p) => appender.append(p.as_bytes()),
                    Fragment::Include(path) => {
                        let id = self.setup.path_to_id(&path).unwrap(); // already checked above
                        let data = resolver.state[&id].unwrap_resolved();
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
            *resolver.state.get_mut(&idx).unwrap() = ResolvingAsset::Resolved(bytes);
        }

        Ok(Some(resolver.state[&start_idx].unwrap_resolved().clone()))
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

    #[error("cyclic include detected when starting with file {0}")]
    CyclicInclude(String),

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
