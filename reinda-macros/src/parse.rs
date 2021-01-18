use std::{collections::HashMap, unreachable};

use proc_macro2::Span;
use syn::{parse::{Parse, ParseStream}, punctuated::Punctuated};

use crate::{Asset, Input};


impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let entries = input.call(<Punctuated<Entry, syn::Token![,]>>::parse_terminated)?;
        let assets = entries.into_iter()
            .map(|entry| {
                Ok((entry.key.value(), attrs_to_asset(entry.attrs)?))
            })
            .collect::<Result<_, syn::Error>>()?;

        Ok(Self { assets })
    }
}

fn attrs_to_asset(attrs: Vec<Attr>) -> Result<Asset, syn::Error> {
    let mut asset = Asset::default();
    let mut hash_span = None;
    for attr in attrs {
        match attr.kind {
            AttrKind::Hash(v) => {
                asset.hash = v;
                hash_span = Some(attr.span);
            }
            AttrKind::Serve(v) => asset.serve = v,
            AttrKind::Template(v) => asset.template = v,
            AttrKind::Prepend(s) => asset.prepend = Some(s),
            AttrKind::Append(s) => asset.append = Some(s),
        }
    }

    if !asset.serve && asset.hash {
        return Err(syn::Error::new(
            hash_span.unwrap(),
            "a hashed file name does not make sense for an asset that is not served",
        ))
    }

    Ok(asset)
}

/// This is the AST that is used for parsing. It never leaves this module,
/// though, as it's quickly converted into the `Input` representation.
struct Entry {
    key: syn::LitStr,
    attrs: Vec<Attr>,
}

struct Attr {
    span: Span,
    kind: AttrKind,
}

enum AttrKind {
    Hash(bool),
    Serve(bool),
    Template(bool),
    Append(String),
    Prepend(String),
}

impl AttrKind {
    fn keyword(&self) -> &'static str {
        match self {
            Self::Hash(_) => "hash",
            Self::Serve(_) => "serve",
            Self::Template(_) => "template",
            Self::Append(_) => "append",
            Self::Prepend(_) => "prepend",
        }
    }
}

impl Parse for Attr {
    fn parse(mut input: ParseStream) -> Result<Self, syn::Error> {
        let ident = input.parse::<syn::Ident>()?;
        let kind = match &*ident.to_string() {
            "hash" => AttrKind::Hash(parse_opt_bool(&mut input)?),
            "serve" => AttrKind::Serve(parse_opt_bool(&mut input)?),
            "template" => AttrKind::Template(parse_opt_bool(&mut input)?),
            k @ "prepend" | k @ "append" => {
                let _: syn::Token![:] = input.parse()?;
                let s = input.parse::<syn::LitStr>()?.value();
                match k {
                    "prepend" => AttrKind::Prepend(s),
                    "append" => AttrKind::Append(s),
                    _ => unreachable!(),
                }
            }
            other => {
                let msg = format!("'{}' not expected here", other);
                return Err(syn::Error::new(ident.span(), msg));
            }
        };

        Ok(Self {
            span: ident.span(),
            kind,
        })
    }
}

fn parse_opt_bool(input: &mut ParseStream) -> Result<bool, syn::Error> {
    if input.peek(syn::Token![:]) {
        let _: syn::Token![:] = input.parse()?;
        let v: syn::LitBool = input.parse()?;
        Ok(v.value)
    } else {
        Ok(true)
    }
}

impl Parse for Entry {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let key = input.parse()?;
        let _: syn::Token![:] = input.parse()?;

        let inner;
        syn::braced!(inner in input);
        let attrs = inner.call(<Punctuated<Attr, syn::Token![,]>>::parse_terminated)?;

        // Check for duplicate attributes
        let mut map = HashMap::new();
        for attr in &attrs {
            if let Some(prev) = map.insert(attr.kind.keyword(), attr.span) {
                let msg = format!(
                    "duplicate specifier: '{}' already listed for this asset",
                    attr.kind.keyword(),
                );
                return Err(syn::Error::new(prev, msg));
            }
        }

        Ok(Self {
            key,
            attrs: attrs.into_iter().collect(),
        })
    }
}
