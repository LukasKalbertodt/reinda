use std::{convert::TryInto, fs::File, io::Read, path::Path};

use proc_macro::TokenStream as TokenStream1;
use proc_macro2::{Span, TokenStream};
use quote::quote;

mod parse;



// See documentation in the main crate.
#[proc_macro]
pub fn assets(input: TokenStream1) -> TokenStream1 {
    run(input.into())
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}


fn run(input: TokenStream) -> Result<TokenStream, syn::Error> {
    let input = syn::parse2::<Input>(input)?;

    let mut match_arms = Vec::new();
    let mut asset_defs = Vec::new();

    for asset in input.assets {
        let path = &asset.path;
        let idx: u32 = match_arms.len().try_into().expect("you have more than 2^32 assets?!");
        match_arms.push(quote! {
            #path => Some(reinda::AssetId(#idx)),
        });

        let hash = match &asset.settings.hash {
            None => quote! { None },
            Some(Some((a, b))) => quote! { Some((#a, #b)) },
            Some(None) => {
                let filename = Path::new(path)
                    .file_name()
                    .expect("no filename in path")
                    .to_str()
                    .unwrap();

                let (a, b) = match filename.find('.') {
                    Some(pos) => (
                        format!("{}.", &filename[..pos]),
                        &filename[pos..]
                    ),
                    None => (
                        format!("{}-", &filename),
                        "",
                    )
                };

                quote! { Some((#a, #b)) }
            }
        };

        let serve = asset.settings.serve;
        let template = asset.settings.template;
        let dynamic = asset.settings.dynamic;
        let append = match &asset.settings.append {
            Some(s) => quote! { Some(#s) },
            None => quote! { None },
        };
        let prepend = match &asset.settings.prepend {
            Some(s) => quote! { Some(#s) },
            None => quote! { None },
        };
        let content_field = if cfg!(all(debug_assertions, not(feature = "debug_is_prod"))) {
            quote! {}
        } else {
            let data = embed(input.base_path.as_deref(), &asset)?;
            quote! { content: #data }
        };

        asset_defs.push(quote! {
            reinda::AssetDef {
                path: #path,
                serve: #serve,
                hash: #hash,
                dynamic: #dynamic,
                template: #template,
                append: #append,
                prepend: #prepend,
                #content_field
            }
        });
    }

    let base_path = &input.base_path.as_ref().ok_or(syn::Error::new(
        Span::call_site(),
        "`base_path` is not set. Please add `#![base_path = \"...\"]` to the top \
            of this macro invocation.",
    ))?;

    Ok(quote! {
        reinda::Setup {
            base_path: #base_path,
            assets: &[#( #asset_defs ,)*],
            path_to_id: reinda::PathToIdMap(|s: &str| -> Option<reinda::AssetId> {
                match s {
                    #( #match_arms )*
                    _ => None,
                }
            }),
        }
    })
}

#[derive(Debug)]
struct Input {
    base_path: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Debug)]
struct Asset {
    path: String,
    path_span: Span,
    settings: AssetSettings,
}

#[derive(Debug)]
struct AssetSettings {
    serve: bool,
    dynamic: bool,
    template: bool,
    hash: Option<Option<(String, String)>>,
    append: Option<String>,
    prepend: Option<String>,
}

impl Default for AssetSettings {
    fn default() -> Self {
        Self {
            serve: true,
            dynamic: false,
            template: false,
            hash: None,
            append: None,
            prepend: None,
        }
    }
}

fn embed(
    base: Option<&str>,
    asset: &Asset,
) -> Result<TokenStream, syn::Error> {
    if asset.settings.dynamic {
        return Ok(quote! { b"" });
    }

    let path = match base {
        Some(base) => {
            let manifest = std::env::var("CARGO_MANIFEST_DIR")
                .expect("CARGO_MANIFEST_DIR not set");
            format!("{}/{}/{}", manifest, base, &asset.path)
        },
        None => asset.path.to_string(),
    };

    // Start with the "prepend" data, if any.
    let mut data = Vec::new();
    if let Some(prepend) = &asset.settings.prepend {
        data.extend_from_slice(prepend.as_bytes());
    }

    // Read the full file.
    let mut file = File::open(&path).map_err(|e| {
        let msg = format!("could not open '{}': {}", path, e);
        syn::Error::new(asset.path_span, msg)
    })?;
    file.read_to_end(&mut data).map_err(|e| {
        let msg = format!("could not read '{}': {}", path, e);
        syn::Error::new(asset.path_span, msg)
    })?;

    // Add the "append" data, if any.
    if let Some(append) = &asset.settings.append {
        data.extend_from_slice(append.as_bytes());
    }


    let lit = syn::LitByteStr::new(&data, Span::call_site());
    Ok(quote! {
        {
            // This is to make cargo/the compiler understand that we want to be
            // recompiled if that file changes.
            include_bytes!(#path);

            #lit
        }
    })
}
