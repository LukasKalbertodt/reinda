use std::{collections::HashMap, unreachable};

use proc_macro2::Span;
use syn::{parse::{Parse, ParseStream}, punctuated::Punctuated, spanned::Spanned};

use crate::{Asset, AssetSettings, Input};


impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        // Parse attributes
        let mut base_path = None;
        if input.peek(syn::Token![#]) {
            let attrs = input.call(syn::Attribute::parse_inner)?;
            for attr in attrs {
                let meta = attr.parse_meta()?;

                match () {
                    () if meta.path().is_ident("base_path") => {
                        if let syn::Meta::NameValue(
                            syn::MetaNameValue { lit: syn::Lit::Str(s), ..}
                        ) = meta {
                            base_path = Some(s.value());
                        } else {
                            return Err(syn::Error::new(
                                meta.span(),
                                "this attribute has the form `#![base_path = \"foo\"]`",
                            ));
                        }
                    }
                    _ => return Err(syn::Error::new(meta.path().span(), "invalid attribute")),
                }

            }
        }

        // Parse list of entries/assets
        let entries = input.call(<Punctuated<Entry, syn::Token![,]>>::parse_terminated)?;
        let assets = entries.into_iter()
            .map(|entry| Ok(Asset {
                path: entry.key.value(),
                path_span: entry.key.span(),
                settings: fields_to_settings(entry.fields)?,
            }))
            .collect::<Result<_, syn::Error>>()?;

        Ok(Self { assets, base_path })
    }
}

fn fields_to_settings(fields: Vec<Field>) -> Result<AssetSettings, syn::Error> {
    let mut asset = AssetSettings::default();
    let mut hash_span = None;
    for field in fields {
        match field.kind {
            FieldKind::Hash(v) => {
                asset.hash = v;
                hash_span = Some(field.span);
            }
            FieldKind::Serve(v) => asset.serve = v,
            FieldKind::Dynamic(v) => asset.dynamic = v,
            FieldKind::Template(v) => asset.template = v,
            FieldKind::Prepend(s) => asset.prepend = Some(s),
            FieldKind::Append(s) => asset.append = Some(s),
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
    fields: Vec<Field>,
}

struct Field {
    span: Span,
    kind: FieldKind,
}

enum FieldKind {
    Hash(bool),
    Serve(bool),
    Dynamic(bool),
    Template(bool),
    Append(String),
    Prepend(String),
}

impl FieldKind {
    fn keyword(&self) -> &'static str {
        match self {
            Self::Hash(_) => "hash",
            Self::Serve(_) => "serve",
            Self::Dynamic(_) => "dynamic",
            Self::Template(_) => "template",
            Self::Append(_) => "append",
            Self::Prepend(_) => "prepend",
        }
    }
}

impl Parse for Field {
    fn parse(mut input: ParseStream) -> Result<Self, syn::Error> {
        let ident = input.parse::<syn::Ident>()?;
        let kind = match &*ident.to_string() {
            "hash" => FieldKind::Hash(parse_opt_bool(&mut input)?),
            "serve" => FieldKind::Serve(parse_opt_bool(&mut input)?),
            "dynamic" => FieldKind::Dynamic(parse_opt_bool(&mut input)?),
            "template" => FieldKind::Template(parse_opt_bool(&mut input)?),
            k @ "prepend" | k @ "append" => {
                let _: syn::Token![:] = input.parse()?;
                let s = input.parse::<syn::LitStr>()?.value();
                match k {
                    "prepend" => FieldKind::Prepend(s),
                    "append" => FieldKind::Append(s),
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
        let fields = inner.call(<Punctuated<Field, syn::Token![,]>>::parse_terminated)?;

        // Check for duplicate fields
        let mut map = HashMap::new();
        for field in &fields {
            if let Some(prev) = map.insert(field.kind.keyword(), field.span) {
                let msg = format!(
                    "duplicate specifier: '{}' already listed for this asset",
                    field.kind.keyword(),
                );
                return Err(syn::Error::new(prev, msg));
            }
        }

        Ok(Self {
            key,
            fields: fields.into_iter().collect(),
        })
    }
}
