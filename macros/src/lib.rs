use proc_macro::TokenStream as TokenStream1;

use self::{
    err::{Error, err},
    ast::{EmbedConfig, Input},
};

mod emit;
mod err;
mod ast;
mod parse;


// See documentation in the main crate.
#[proc_macro]
pub fn embed(input: TokenStream1) -> TokenStream1 {
    parse::parse(input.into())
        .and_then(emit::emit)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
