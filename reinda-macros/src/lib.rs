use std::collections::HashMap;

use proc_macro::TokenStream as TokenStream1;
use proc_macro2::TokenStream;
use syn::Error;
use quote::quote;

mod parse;


#[proc_macro]
pub fn assets(input: TokenStream1) -> TokenStream1 {
    run(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}


fn run(input: TokenStream) -> Result<TokenStream, Error> {

    Ok(quote! { const FOO: u32 = 3; })
}

struct Input {
    serve: HashMap<String, ServedAsset>,
    includes: HashMap<String, IncludedAsset>,
}

struct ServedAsset {
    hash: bool,
    mods: Modifications,
}

struct IncludedAsset {
    mods: Modifications,
}

struct Modifications {
    template: bool,
    append: Option<String>,
    prepend: Option<String>,
}
