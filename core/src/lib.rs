use std::fmt;

pub mod template;


/// Simple ID to refer to one asset in a `Setup` or `Assets` struct.
///
/// **Note**: the field of this struct is public such that it can be created by
/// the `assets!` macro. However, you must not create instances of `AssetId`
/// yourself or access this field.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetId(#[doc(hidden)] pub u32);

// Manual implementation to easy debugging. In pretty print debug output, it
// really does not make sense to put the field in its own line.
impl fmt::Debug for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "AssetId({})", self.0)
    }
}


// See documentation in the main crate.
#[derive(Debug, Clone, Copy)]
pub struct Setup {
    #[doc(hidden)]
    pub assets: &'static [AssetDef],
    #[doc(hidden)]
    pub path_to_id: PathToIdMap,
    #[doc(hidden)]
    pub base_path: &'static str,
}

impl Setup {
    #[doc(hidden)]
    pub fn asset_by_path(&self, path: &str) -> Option<&AssetDef> {
        self.path_to_id(path).map(|id| self.def(id))
    }

    #[doc(hidden)]
    pub fn path_to_id(&self, path: &str) -> Option<AssetId> {
        (self.path_to_id.0)(path)
    }

    #[doc(hidden)]
    pub fn def(&self, id: AssetId) -> &AssetDef {
        &self.assets[id.0 as usize]
    }
}

#[derive(Clone, Copy)]
#[doc(hidden)]
pub struct PathToIdMap(pub fn(&str) -> Option<AssetId>);

impl fmt::Debug for PathToIdMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("<function>")
    }
}

#[derive(Debug, Clone, Copy)]
#[doc(hidden)]
pub struct AssetDef {
    pub path: &'static str,

    pub serve: bool,
    pub dynamic: bool,

    /// Contains two strings in between which the hash should be inserted.
    pub hash: Option<(&'static str, &'static str)>,
    pub template: bool,
    pub append: Option<&'static str>,
    pub prepend: Option<&'static str>,

    #[cfg(any(not(debug_assertions), feature = "debug_is_prod"))]
    pub content: &'static [u8],
}

impl AssetDef {
    /// Returns whether or not this asset's filename should include a hash. In
    /// prod mode, this returns `self.hash`, in dev mode this always returns
    /// `false`.
    pub fn hashed_filename(&self) -> bool {
        self.hash.is_some() && cfg!(any(not(debug_assertions), feature = "debug_is_prod"))
    }
}
