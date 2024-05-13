use std::{borrow::Cow, path::PathBuf, sync::Arc};

use bytes::Bytes;

use crate::{Assets, BuildError, DataSource, EmbeddedEntry, EmbeddedFile, EmbeddedGlob, Modifier, ModifierContext, PathHash, SplitGlob};


/// Helper to build [`Assets`].
#[derive(Debug)]
pub struct Builder<'a> {
    pub(crate) assets: Vec<EntryBuilder<'a>>,
}

#[derive(Debug)]
pub struct EntryBuilder<'a> {
    pub(crate) kind: EntryBuilderKind<'a>,
    #[cfg_attr(not(feature = "hash"), allow(dead_code))]
    pub(crate) path_hash: PathHash<'a>,
    pub(crate) modifier: Modifier,
}

#[derive(Debug)]
pub(crate) enum EntryBuilderKind<'a> {
    Single {
        http_path: &'a str,
        source: DataSource,
    },
    Glob {
        http_prefix: &'a str,
        #[cfg_attr(prod_mode, allow(dead_code))]
        glob: SplitGlob,
        files: Vec<GlobFile>,
        #[cfg(dev_mode)]
        base_path: &'static str,
    }
}

#[derive(Debug)]
pub(crate) struct GlobFile {
    pub(crate) suffix: &'static str,
    pub(crate) source: DataSource,
}

impl<'a> Builder<'a> {
    pub fn add_file(
        &mut self,
        http_path: &'a str,
        fs_path: impl Into<PathBuf>,
    ) -> &mut EntryBuilder<'a> {
        self.assets.push(EntryBuilder {
            kind: EntryBuilderKind::Single {
                http_path,
                source: DataSource::File(fs_path.into()),
            },
            path_hash: PathHash::None,
            modifier: Modifier::None,
        });
        self.assets.last_mut().unwrap()
    }

    pub fn add_embedded_file(
        &mut self,
        http_path: &'a str,
        file: &EmbeddedFile,
    ) -> &mut EntryBuilder<'a> {
        self.assets.push(EntryBuilder {
            kind: EntryBuilderKind::Single {
                http_path,
                source: file.data_source(),
            },
            path_hash: PathHash::None,
            modifier: Modifier::None,
        });
        self.assets.last_mut().unwrap()
    }

    pub fn add_embedded_glob(
        &mut self,
        http_path: &'a str,
        glob: &'a EmbeddedGlob,
    ) -> &mut EntryBuilder<'a> {
        let split_glob = SplitGlob::new(glob.pattern);
        self.assets.push(EntryBuilder {
            kind: EntryBuilderKind::Glob {
                http_prefix: http_path,
                files: glob.files.iter().map(|f| GlobFile {
                    // This should never be `None`
                    suffix: f.path.strip_prefix(&split_glob.prefix)
                        .expect("embedded file path does not start with glob prefx"),
                    source: f.data_source(),
                }).collect(),
                glob: split_glob,
                #[cfg(dev_mode)]
                base_path: glob.base_path,
            },
            path_hash: PathHash::None,
            modifier: Modifier::None,
        });
        self.assets.last_mut().unwrap()
    }

    pub fn add_embedded(
        &mut self,
        http_path: &'a str,
        entry: &'a EmbeddedEntry,
    ) -> &mut EntryBuilder<'a> {
        match entry {
            EmbeddedEntry::Single(file) => self.add_embedded_file(http_path, file),
            EmbeddedEntry::Glob(glob) => self.add_embedded_glob(http_path, glob),
        }
    }

    pub async fn build(self) -> Result<Assets, BuildError> {
        crate::imp::AssetsInner::build(self).await.map(Assets)
    }
}

impl<'a> EntryBuilder<'a> {
    #[cfg(feature = "hash")]
    pub fn with_hash(&mut self) -> &mut Self {
        self.path_hash = PathHash::Auto;
        self
    }

    #[cfg(feature = "hash")]
    pub fn with_hash_between(&mut self, prefix: &'a str, suffix: &'a str) -> &mut Self {
        self.path_hash = PathHash::InBetween { prefix, suffix };
        self
    }

    pub fn with_modifier<F, D, T>(&mut self, dependencies: D, modifier: F) -> &mut Self
    where
        F: 'static + Send + Sync + Fn(Bytes, ModifierContext) -> Bytes,
        D: IntoIterator<Item = T>,
        T: Into<Cow<'static, str>>,
    {
        self.modifier = Modifier::Custom {
            f: Arc::new(modifier),
            deps: dependencies.into_iter().map(Into::into).collect(),
        };
        self
    }
}
