use std::{convert::TryFrom, iter::Peekable};
use proc_macro2::{token_stream::IntoIter, Delimiter, TokenStream, TokenTree};

use crate::{err::{err, Error}, ast::Input};


pub(crate) fn parse(tokens: TokenStream) -> Result<Input, Error> {
    let mut base_path = None;
    let mut files = None;
    let mut compression_threshold = None;
    let mut compression_quality = None;
    let mut print_stats = None;

    let mut it = tokens.into_iter().peekable();

    while it.peek().is_some() {
        // Parse field name.
        let field_name = match it.next().unwrap() {
            TokenTree::Ident(i) => i,
            other => return Err(err!(@other.span(), "expected identifier, found something else")),
        };

        // Parse `:`
        match it.next().ok_or_else(unexpected_end_of_input)? {
            TokenTree::Punct(p) if p.as_char() == ':' => {}
            other => return Err(err!(@other.span(), "expected `:`, found something else")),
        }

        // Parse value.
        match field_name.to_string().as_str() {
            "base_path" => {
                base_path = Some(parse_string_lit(&mut it)?);
            }

            "print_stats" => {
                print_stats = Some(parse_lit::<litrs::BoolLit>(&mut it)?.value());
            }

            "compression_threshold" => {
                let lit = parse_lit::<litrs::FloatLit<String>>(&mut it)?;
                let value = lit.number_part().parse()
                    .map_err(|e| err!("failed to parse compression_threshold: {e}"))?;
                compression_threshold = Some(value);
            }

            "compression_quality" => {
                let lit = parse_lit::<litrs::IntegerLit<String>>(&mut it)?;
                let value = lit.value::<u8>()
                    .ok_or_else(|| err!("compression quality too large"))?;
                compression_quality = Some(value);
            }

            "files" => {
                let inner = match it.next().ok_or_else(unexpected_end_of_input)? {
                    TokenTree::Group(g) if g.delimiter() == Delimiter::Bracket => g.stream(),
                    other => return Err(err!(@other.span(), "expected string array `[...]`")),
                };

                let mut inner_it = inner.into_iter().peekable();
                let mut values = vec![];
                while inner_it.peek().is_some() {
                    let span = inner_it.peek().unwrap().span();
                    let value = parse_string_lit(&mut inner_it)?;
                    values.push((value, span));
                    eat_comma_sep(&mut inner_it)?;
                }

                files = Some(values);
            }

            other => return Err(err!(@field_name.span(), "unknown field name '{other}'")),
        }

        eat_comma_sep(&mut it)?;
    }

    Ok(Input {
        base_path,
        print_stats,
        compression_threshold,
        compression_quality,
        files: files.ok_or_else(|| err!("missing field 'files' in input"))?,
    })
}

fn unexpected_end_of_input() -> Error {
    err!("unexpected end of input")
}

type ParseIter = Peekable<IntoIter>;

fn eat_comma_sep(it: &mut ParseIter) -> Result<(), Error> {
    match it.next() {
        None => Ok(()),
        Some(TokenTree::Punct(p)) if p.as_char() == ',' => Ok(()),
        Some(other) => Err(err!(@other.span(), "expected comma or end of input")),
    }
}

fn parse_string_lit(it: &mut ParseIter) -> Result<String, Error> {
    parse_lit::<litrs::StringLit<String>>(it).map(|l| l.into_value().into_owned())
}

fn parse_lit<T>(it: &mut ParseIter) -> Result<T, Error>
where
    T: TryFrom<TokenTree>,
    T::Error: std::fmt::Display,
{
    let token = it.next().ok_or_else(unexpected_end_of_input)?;
    let span = token.span();
    T::try_from(token).map_err(|e| err!(@span, "{e}"))
}
