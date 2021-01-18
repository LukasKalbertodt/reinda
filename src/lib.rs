use std::{collections::HashMap, path::{Path, PathBuf}};
use bytes::Bytes;

use reinda_core::template;
pub use reinda_macros::assets;
pub use reinda_core::{AssetDef, Setup};


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
                match (self.setup.path_to_idx)(path) {
                    Some(idx) => Bytes::from_static(self.setup.assets[idx].content),
                    None => return Ok(None),
                }
            }
        };

        let mut unresolved_fragments = Vec::new();
        for span in template::FragmentSpans::new(&content) {
            let raw = &content[span];
            let s = std::str::from_utf8(raw)
                .map_err(|_| Error::NonUtf8TemplateFragment(raw.into()))?
                .trim();

            unresolved_fragments.push(Fragment::from_str(s)?);
        }

        Ok(Some(RawAsset {
            content,
            unresolved_fragments,
        }))
    }
}

/// All errors that might be returned by `reinda`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("template fragment does not contain valid UTF8: {0:?}")]
    NonUtf8TemplateFragment(Vec<u8>),

    #[error("unknown template fragment specifier {0}")]
    UnknownTemplateSpecifier(String),
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
    fn from_str(s: &str) -> Result<Self, Error> {
        let val = |s: &str| s[s.find(':').unwrap() + 1..].to_string();

        match () {
            () if s.starts_with("path:") => Ok(Self::Path(val(s))),
            () if s.starts_with("include:") => Ok(Self::Include(val(s))),
            () if s.starts_with("var:") => Ok(Self::Var(val(s))),

            _ => {
                let key = s[..s.find(':').unwrap_or(s.len())].to_string();
                Err(Error::UnknownTemplateSpecifier(key))
            }
        }
    }
}
