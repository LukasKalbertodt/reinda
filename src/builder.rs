use std::{
    borrow::Cow,
    path::{Path, PathBuf},
    sync::Arc,
};

use bytes::Bytes;

use crate::{
    Assets, BuildError, DataSource, EmbeddedEntry, EmbeddedFile, EmbeddedGlob, Modifier,
    ModifierContext, PathHash, SplitGlob,
};

/// Helper to build [`Assets`].
#[derive(Debug)]
pub struct Builder<'a> {
    pub(crate) assets: Vec<EntryBuilder<'a>>,
}

/// Returned by the various `Builder::add_*` functions, allowing you to
/// configure added assets.
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
        http_path: Cow<'a, str>,
        source: DataSource,
    },
    Glob {
        http_prefix: Cow<'a, str>,
        #[cfg_attr(prod_mode, allow(dead_code))]
        glob: SplitGlob,
        files: Vec<GlobFile>,
        #[cfg(dev_mode)]
        base_path: &'static str,
    },
}

#[derive(Debug)]
pub(crate) struct GlobFile {
    pub(crate) suffix: &'static str,
    pub(crate) source: DataSource,
}

impl<'a> Builder<'a> {
    /// Adds an asset by *FS path*, to be loaded at runtime (instead of being
    /// embedded into the executable). In prod mode, this is loaded in
    /// `Builder::build`. Mounts it under the given HTTP path.
    pub fn add_file(
        &mut self,
        http_path: impl Into<Cow<'a, str>>,
        fs_path: impl Into<PathBuf>,
    ) -> &mut EntryBuilder<'a> {
        self.assets.push(EntryBuilder {
            kind: EntryBuilderKind::Single {
                http_path: http_path.into(),
                source: DataSource::File(fs_path.into()),
            },
            path_hash: PathHash::None,
            modifier: Modifier::None,
        });
        self.assets.last_mut().unwrap()
    }

    /// Adds an embedded entry (single file or glob). Just calls
    /// [`Self::add_embedded_file`] or [`Self::add_embedded_glob`], depending
    /// on `entry`. See those functions for more information.
    pub fn add_embedded(
        &mut self,
        http_path: impl Into<Cow<'a, str>>,
        entry: &'a EmbeddedEntry,
    ) -> &mut EntryBuilder<'a> {
        match entry {
            EmbeddedEntry::Single(file) => self.add_embedded_file(http_path, file),
            EmbeddedEntry::Glob(glob) => self.add_embedded_glob(http_path, glob),
        }
    }

    /// Adds an embedded file and mounts it under the given HTTP path.
    pub fn add_embedded_file(
        &mut self,
        http_path: impl Into<Cow<'a, str>>,
        file: &EmbeddedFile,
    ) -> &mut EntryBuilder<'a> {
        self.assets.push(EntryBuilder {
            kind: EntryBuilderKind::Single {
                http_path: http_path.into(),
                source: file.data_source(),
            },
            path_hash: PathHash::None,
            modifier: Modifier::None,
        });
        self.assets.last_mut().unwrap()
    }

    /// Adds an embedded glob. All files matching this glob are mounted with
    /// `http_path` as prefix. Specifically, all leading glob segments that do
    /// not contain glob characters are stripped, and `http_path` is prefixed
    /// in front of the matching files.
    ///
    /// For example:
    /// - Consider the following files: `foo/bar/cat.svg`, `foo/bar/dog.svg`,
    ///   `foo/not-matching.txt`.
    /// - In `embed!` you specify `foo/bar/*.svg`.
    /// - This matches the two SVG files: `foo/bar/cat.svg` and `foo/bar/dog.svg`.
    /// - Then you call `add_embedded_glob("animals/", &EMBEDS["foo/bar/*.svg"])`.
    /// - The leading non-glob segments of the glob (`foo/bar/*.svg`) are
    ///   `foo/bar/`, which are removed from the matched files: `cat.svg` and
    ///   `dog.svg`.
    /// - Finally, the specified `http_path` (`animals/`) is prefixed, resulting
    ///   in: `animals/cat.svg` and `animals/dog.svg`.
    ///
    /// This might sound complicated but should be fairly straight forward and
    /// is, I think, the must useful in practice.
    pub fn add_embedded_glob(
        &mut self,
        http_path: impl Into<Cow<'a, str>>,
        glob: &'a EmbeddedGlob,
    ) -> &mut EntryBuilder<'a> {
        let split_glob = SplitGlob::new(glob.pattern);
        self.assets.push(EntryBuilder {
            kind: EntryBuilderKind::Glob {
                http_prefix: http_path.into(),
                files: glob
                    .files
                    .iter()
                    .map(|f| {
                        GlobFile {
                            // This should never be `None`
                            suffix: Path::new(f.path)
                                .strip_prefix(split_glob.prefix)
                                .expect("embedded file path does not start with glob prefix")
                                .to_str()
                                .expect("embedded file path contains invalid UTF-8 characters"),
                            source: f.data_source(),
                        }
                    })
                    .collect(),
                glob: split_glob,
                #[cfg(dev_mode)]
                base_path: glob.base_path,
            },
            path_hash: PathHash::None,
            modifier: Modifier::None,
        });
        self.assets.last_mut().unwrap()
    }

    /// Builds `Assets` from the configured assets. In prod mode, everything is
    /// loaded, processed, and assembled into a fast data structure. In dev
    /// mode, those steps are deferred to later.
    pub async fn build(self) -> Result<Assets, BuildError> {
        crate::imp::AssetsInner::build(self).await.map(Assets)
    }
}

impl<'a> EntryBuilder<'a> {
    /// Adds the hash of this asset's content to its HTTP filename (in prod mode).
    ///
    /// This helps a lot with caching on the web. If you include the content
    /// hash in the filename then you can set very strong caching headers, like
    /// `Cache-control :public, max-age=31536000, immutable`, meaning that the
    /// browser can cache that file for essentially infinitely long time
    /// without relying on `If-Modified-Since` or `E-Tag` headers.
    ///
    /// In dev mode, hashes are never inserted.
    ///
    /// The hash is inserted after the first `.` in the filename and an
    /// additional `.` is added after the hash. Example: `bundle.js.map`
    /// becomes `bundle.sbfNUtVcqxUK.js.map`. If there is no `.` in the
    /// filename, a `-` and then the hash is appended, e.g. `foo-sbfNUtVcqxUK`.
    ///
    /// Method is only available if the crate feature `hash` is enabled.
    #[cfg(feature = "hash")]
    pub fn with_hash(&mut self) -> &mut Self {
        self.path_hash = PathHash::Auto;
        self
    }

    // TODO: make public again once its tested.
    /// Like [`Self::with_hash`], but lets you specify where it insert the hash.
    #[cfg(feature = "hash")]
    #[allow(dead_code)]
    fn with_hash_between(&mut self, prefix: &'a str, suffix: &'a str) -> &mut Self {
        self.path_hash = PathHash::InBetween { prefix, suffix };
        self
    }

    /// Replaces occurences of any of the given *unhashed HTTP paths* in this
    /// asset with the corresponding *hashed HTTP path*. This is a specialized
    /// version of [`Self::with_modifier`].
    pub fn with_path_fixup<D, T>(&mut self, paths: D) -> &mut Self
    where
        D: IntoIterator<Item = T>,
        T: Into<Cow<'static, str>>,
    {
        self.modifier = Modifier::PathFixup(paths.into_iter().map(Into::into).collect());
        self
    }

    /// Registers a modifier that modifies this asset's content, being able to
    /// resolve *unhashed HTTP paths* to *hashed HTTP paths*.
    ///
    /// If you just need to replace paths, [`Self::with_path_fixup`] might work
    /// for you. This is the more powerful version, allowing you to perform
    /// arbitrary logic with the asset's content. In prod mode, this is called
    /// once when you call [`Builder::build`]; in dev mode, it's called every
    /// time the asset is loaded.
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

    /// Returns all *unhashed HTTP paths* that are mounted by this entry. This
    /// is mainly useful to pass as dependencies to [`Self::with_modifier`] or
    /// [`Self::with_path_fixup`] of another entry.
    pub fn http_paths(&self) -> Vec<Cow<'a, str>> {
        match &self.kind {
            EntryBuilderKind::Single { http_path, .. } => {
                vec![http_path.clone()]
            }
            EntryBuilderKind::Glob {
                http_prefix, files, ..
            } => files
                .iter()
                .map(|f| f.http_path(http_prefix).into())
                .collect(),
        }
    }

    /// Like [`Self::http_paths`] but asserting that there is only one path
    /// added by this entry. If that's not the case, `None` is returned.
    pub fn single_http_path(&self) -> Option<Cow<'a, str>> {
        match &self.kind {
            EntryBuilderKind::Single { http_path, .. } => Some(http_path.clone()),
            EntryBuilderKind::Glob {
                http_prefix, files, ..
            } => {
                if files.len() == 1 {
                    Some(files[0].http_path(http_prefix).into())
                } else {
                    None
                }
            }
        }
    }
}

impl GlobFile {
    pub(crate) fn http_path(&self, http_prefix: &str) -> String {
        // suffix is split from EmbeddedFile::path
        // we trust it should be a valid path string
        // so we only need to replace the backslashes with slashes
        format!("{http_prefix}{}", self.suffix.replace('\\', "/"))
    }
}
