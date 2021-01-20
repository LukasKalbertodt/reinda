use std::{fmt, ops};

pub mod template;


/// Structure that holds metadata and (in release mode) the included raw asset
/// data.
///
/// The fields of this struct are public such that it can be const-constructed,
/// but you shouldn't really access the fields yourself.
#[derive(Debug, Clone, Copy)]
pub struct Setup {
    pub assets: &'static [AssetDef],
    pub path_to_id: PathToIdMap,
    pub base_path: &'static str,
}

impl Setup {
    pub fn asset_by_path(&self, path: &str) -> Option<&AssetDef> {
        self.path_to_id(path).map(|id| &self[id])
    }

    pub fn path_to_id(&self, path: &str) -> Option<AssetId> {
        (self.path_to_id.0)(path)
    }
}

impl ops::Index<AssetId> for Setup {
    type Output = AssetDef;
    fn index(&self, id: AssetId) -> &AssetDef {
        &self.assets[id.0 as usize]
    }
}

#[derive(Clone, Copy)]
pub struct PathToIdMap(pub fn(&str) -> Option<AssetId>);

impl fmt::Debug for PathToIdMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("<function>")
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AssetDef {
    pub path: &'static str,

    pub serve: bool,
    pub dynamic: bool,
    pub hash: bool, // TODO
    pub template: bool,
    pub append: Option<&'static str>,
    pub prepend: Option<&'static str>,

    #[cfg(not(debug_assertions))]
    pub content: &'static [u8],
}

/// Simple ID to refer to one asset in one `Setup` or `Assets` struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AssetId(pub u32);
