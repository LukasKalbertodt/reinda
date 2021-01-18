use std::fmt;

pub mod template;



#[derive(Clone, Copy)]
pub struct Setup {
    pub assets: &'static [AssetDef],
    pub path_to_idx: fn(&str) -> Option<usize>,
    pub base_path: &'static str,
}

impl fmt::Debug for Setup {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        struct FunctionDummyDebug;
        impl fmt::Debug for FunctionDummyDebug {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("<function>")
            }
        }

        f.debug_struct("Setup")
            .field("assets", &self.assets)
            .field("path_to_idx", &FunctionDummyDebug)
            .finish()
    }
}


#[derive(Debug, Clone, Copy)]
pub struct AssetDef {
    pub path: &'static str,

    pub serve: bool,
    pub hash: bool, // TODO
    pub template: bool,
    pub append: Option<&'static str>,
    pub prepend: Option<&'static str>,

    #[cfg(not(debug_assertions))]
    pub content: &'static [u8],
}
