use std::collections::HashMap;

use proc_macro::TokenStream as TokenStream1;
use proc_macro2::TokenStream;
use quote::quote;

mod parse;


#[proc_macro]
pub fn assets(input: TokenStream1) -> TokenStream1 {
    run(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}


fn run(input: TokenStream) -> Result<TokenStream, syn::Error> {
    let input = syn::parse2::<Input>(input)?;
    println!("{:#?}", input);

    Ok(quote! { const FOO: u32 = 3; })
}

#[derive(Debug)]
struct Input {
    serve: HashMap<String, ServedAsset>,
    includes: HashMap<String, IncludedAsset>,
}

#[derive(Debug)]
struct ServedAsset {
    hash: bool,
    mods: Modifications,
}

#[derive(Debug)]
struct IncludedAsset {
    mods: Modifications,
}

#[derive(Debug)]
struct Modifications {
    template: bool,
    append: Option<String>,
    prepend: Option<String>,
}

impl Default for ServedAsset {
    fn default() -> Self {
        Self {
            hash: false,
            mods: Modifications::default(),
        }
    }
}

impl Default for IncludedAsset {
    fn default() -> Self {
        Self {
            mods: Modifications::default(),
        }
    }
}

impl Default for Modifications {
    fn default() -> Self {
        Self {
            template: false,
            append: None,
            prepend: None,
        }
    }
}
