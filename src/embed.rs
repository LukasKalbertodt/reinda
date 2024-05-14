//! API related to `embed!` macro.

use std::ops;

use crate::DataSource;


/// Collection of files embedded into the executable by [`embed!`][super::embed!].
#[derive(Debug)]
pub struct Embeds {
    #[doc(hidden)]
    pub entries: &'static [EmbeddedEntry],
}

/// Corresponds to one entry in the `files` array specified in
/// [`embed!`][super::embed!], either a single file or a glob.
#[derive(Debug)]
#[non_exhaustive]
pub enum EmbeddedEntry {
    /// A single embedded file. The corresponding entry in the macro did not
    /// contain any glob meta characters (or only escaped ones).
    Single(EmbeddedFile),

    /// An entry in the macro with glob meta characters, matching potentially
    /// multiple files.
    Glob(EmbeddedGlob),
}

/// A glob entry embedded by [`embed!`][super::embed!].
#[derive(Debug)]
pub struct EmbeddedGlob {
    /// The glob pattern that was specified in the macro.
    #[doc(hidden)]
    pub pattern: &'static str,

    /// All files that matched the glob pattern at build time.
    #[doc(hidden)]
    pub files: &'static [EmbeddedFile],

    /// Base path specified in the macro, only used to prefix the `pattern` for
    /// loading files in dev mode.
    #[cfg(dev_mode)]
    #[doc(hidden)]
    pub base_path: &'static str,
}

/// A single file embedded by [`embed!`][super::embed!].
#[derive(Debug)]
pub struct EmbeddedFile {
    #[doc(hidden)]
    pub path: &'static str,

    /// The full absolute path, the same from which the content would be loaded
    /// in prod mode.
    #[cfg(dev_mode)]
    #[doc(hidden)]
    pub full_path: &'static str,

    /// The actual file contents.
    #[cfg(prod_mode)]
    #[doc(hidden)]
    pub content: &'static [u8],

    /// Whether the `content` field is compressed.
    #[cfg(prod_mode)]
    #[doc(hidden)]
    pub compressed: bool,
}

impl Embeds {
    /// Returns all embedded entries, one for each string literal in the `files`
    /// array inside the `embed!` macro.
    pub fn entries(&self) -> impl Iterator<Item = &'static EmbeddedEntry> {
        self.entries.iter()
    }

    /// Returns the entry with the specified *embed pattern* (string specified
    /// in the macro). You can also use the index operator `EMBEDS["foo.txt"]`
    /// which works like this method, but panics if no entry with the specified
    // *embed pattern* is found.
    pub fn get(&self, embed_pattern: &str) -> Option<&EmbeddedEntry> {
        // Yes this is O(n), but building a better data structure as static data
        // is not trivial and it really doesn't matter in this case.
        self.entries.iter().find(|entry| entry.embed_pattern() == embed_pattern)
    }
}

/// See [`Embeds::get`].
impl ops::Index<&str> for Embeds {
    type Output = EmbeddedEntry;

    fn index(&self, index: &str) -> &Self::Output {
        self.get(index)
            .unwrap_or_else(|| panic!("no embedded entry found with '{}'", index))
    }
}

impl EmbeddedEntry {
    /// Returns the *embed pattern*, which is the path or pattern string
    /// specified in the macro for this entry. That's either
    /// [`EmbeddedFile::path`] or [`EmbeddedGlob::pattern`], depending on the
    /// type of this entry.
    pub fn embed_pattern(&self) -> &'static str {
        match self {
            EmbeddedEntry::Single(f) => f.path(),
            EmbeddedEntry::Glob(g) => g.pattern(),
        }
    }

    /// Returns `Some(_)` if this entry is an embedded glob, `None` otherwise.
    pub fn as_glob(&self) -> Option<&EmbeddedGlob> {
        match self {
            EmbeddedEntry::Glob(g) => Some(g),
            _ => None,
        }
    }

    /// Returns `Some(_)` if this entry is an embedded single file, `None`
    /// otherwise.
    pub fn as_file(&self) -> Option<&EmbeddedFile> {
        match self {
            EmbeddedEntry::Single(f) => Some(f),
            _ => None,
        }
    }

    /// Returns the files in this entry. If it's a single file, the returned
    /// iterator contains one item, otherwise it's like [`EmbeddedGlob::files`].
    pub fn files(&self) -> impl Iterator<Item = &EmbeddedFile> {
        match self {
            EmbeddedEntry::Single(f) => std::slice::from_ref(f).iter(),
            EmbeddedEntry::Glob(glob) => glob.files.iter(),
        }
    }
}

impl From<EmbeddedGlob> for EmbeddedEntry {
    fn from(value: EmbeddedGlob) -> Self {
        Self::Glob(value)
    }
}

impl From<EmbeddedFile> for EmbeddedEntry {
    fn from(value: EmbeddedFile) -> Self {
        Self::Single(value)
    }
}

impl EmbeddedGlob {
    /// The glob pattern, i.e. the exact string specified in the macro.
    pub fn pattern(&self) -> &'static str {
        self.pattern
    }

    /// Iterator over all files matching the glob pattern found at build time.
    pub fn files(&self) -> impl Iterator<Item = &'static EmbeddedFile> {
        self.files.iter()
    }
}

impl EmbeddedFile {
    /// Returns the relative path of the embedded file, with the base path
    /// stripped. When not using glob patterns, this is exactly the string you
    /// specified inside the `embed!` macro.
    ///
    /// Note: the absolute path of this file is not stored, as including this in
    /// the binary might leak information about the build system.
    pub fn path(&self) -> &'static str {
        self.path
    }

    /// Returns the contents of the embedded file. This method might decompress
    /// data, so try calling it only once for each file to avoid doing
    /// duplicate work.
    #[cfg(prod_mode)]
    pub fn content(&self) -> std::borrow::Cow<'static, [u8]> {
        #[cfg(feature = "compress")]
        if self.compressed {
            let mut decompressed = Vec::new();
            brotli::BrotliDecompress(&mut &*self.content, &mut decompressed)
                .expect("unexpected error while decompressing Brotli");
            decompressed.into()
        } else {
            self.content.into()
        }

        #[cfg(not(feature = "compress"))]
        { self.content.into() }
    }

    pub(crate) fn data_source(&self) -> DataSource {
        #[cfg(dev_mode)]
        { DataSource::File(self.full_path.into()) }

        #[cfg(prod_mode)]
        {
            let bytes = match self.content() {
                std::borrow::Cow::Borrowed(slice) => slice.into(),
                std::borrow::Cow::Owned(vec) => vec.into(),
            };
            DataSource::Loaded(bytes)
        }
    }
}
