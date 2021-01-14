use std::{collections::HashMap, unreachable};

use proc_macro2::Span;
use syn::{Ident, parse::{Parse, ParseStream}, punctuated::Punctuated};

use crate::{IncludedAsset, Input, ServedAsset};

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let blocks = input.call(<Punctuated<Block, syn::Token![,]>>::parse_terminated)?;
        let mut blocks = blocks.into_iter().collect::<Vec<_>>();

        let serve = assets_from_block(&mut blocks, "serve")?;
        let includes = assets_from_block(&mut blocks, "includes")?;

        if let Some(unused) = blocks.first() {
            let msg = format!(
                "block with key '{}' not valid (did you mean 'serve' or 'includes'?)",
                unused.name,
            );
            return Err(syn::Error::new(unused.name.span(), msg));
        }

        Ok(Self { serve, includes })
    }
}

fn assets_from_block<A: FromAttrs>(
    blocks: &mut Vec<Block>,
    name: &str,
) -> Result<HashMap<String, A>, syn::Error> {
    blocks.iter()
        .position(|b| b.name == name)
        .map(|pos| {
            let block = blocks.swap_remove(pos);
            block.entries.into_iter()
                .map(|entry| Ok((entry.key.value(), A::from_attrs(entry.attrs)?)))
                .collect::<Result<HashMap<String, A>, syn::Error>>()
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

trait FromAttrs: Sized {
    fn from_attrs(attrs: Vec<Attr>) -> Result<Self, syn::Error>;
}

impl FromAttrs for ServedAsset {
    fn from_attrs(attrs: Vec<Attr>) -> Result<Self, syn::Error> {
        let mut out = Self::default();
        for attr in attrs {
            match attr.kind {
                AttrKind::Hash => out.hash = true,
                AttrKind::Template => out.mods.template = true,
                AttrKind::Prepend(s) => out.mods.prepend = Some(s),
                AttrKind::Append(s) => out.mods.append = Some(s),
            }
        }

        Ok(out)
    }
}

impl FromAttrs for IncludedAsset {
    fn from_attrs(attrs: Vec<Attr>) -> Result<Self, syn::Error> {
        let mut out = Self::default();
        for attr in attrs {
            match attr.kind {
                AttrKind::Hash => {
                    return Err(syn::Error::new(
                        attr.span,
                        "'hash' specifier does not make sense for included assets",
                    ));
                }
                AttrKind::Template => out.mods.template = true,
                AttrKind::Prepend(s) => out.mods.prepend = Some(s),
                AttrKind::Append(s) => out.mods.append = Some(s),
            }
        }

        Ok(out)
    }
}



/// This is the AST that is used for parsing. It never leaves this module,
/// though, as it's quickly converted into the `Input` representation.
struct Block {
    name: Ident,
    entries: Vec<Entry>,
}

struct Entry {
    key: syn::LitStr,
    attrs: Vec<Attr>,
}

struct Attr {
    span: Span,
    kind: AttrKind,
}

enum AttrKind {
    Hash,
    Template,
    Append(String),
    Prepend(String),
}

impl AttrKind {
    fn keyword(&self) -> &'static str {
        match self {
            Self::Hash => "hash",
            Self::Template => "template",
            Self::Append(_) => "append",
            Self::Prepend(_) => "prepend",
        }
    }
}

impl Parse for Attr {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let ident = input.parse::<syn::Ident>()?;
        let kind = match &*ident.to_string() {
            "hash" => AttrKind::Hash,
            "template" => AttrKind::Template,
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

impl Parse for Block {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        let name = input.parse()?;
        let _: syn::Token![:] = input.parse()?;

        let inner;
        syn::braced!(inner in input);
        let entries = inner.call(<Punctuated<_, syn::Token![,]>>::parse_terminated)?;

        Ok(Self {
            name,
            entries: entries.into_iter().collect(),
        })
    }
}
