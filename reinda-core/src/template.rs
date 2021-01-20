use std::ops::Range;
use bytes::Bytes;



const FRAGMENT_START: &[u8] = b"{{: ";
const FRAGMENT_END: &[u8] = b" :}}";

/// Fragments longer than this are ignored. This is just to protect against
/// random fragment start/end markers in large generated files.
pub const MAX_FRAGMENT_LEN: usize = 256;


/// A byte string that you can append to. Used for [`Template::render`].
pub struct Appender<'a>(&'a mut Vec<u8>);

impl Appender<'_> {
    pub fn append(&mut self, s: &[u8]) {
        self.0.extend_from_slice(s);
    }
}

/// A parsed template.
///
/// # Template syntax
///
/// Our template syntax is super simple and is really just a glorified
/// search-and-replace. The input is checked for "fragments" which have the
/// syntax `{{: foo :}}`. The start token is actually `{{: ` (note the
/// whitespace!). So `{{:foo:}}` is not recognized as fragment.
///
/// There are two additional constraints: the fragment must not contain a
/// newline and must be shorter than [`MAX_FRAGMENT_LEN`]. If these conditions
/// are not met, the fragment start token is ignored.
///
/// The string between the start and end tag is then trimmed (excess whitespace
/// removed) and parsed into a [`Fragment`]. See that type's documentation for
/// information about the existing kinds of fragments.
pub struct Template {
    raw: Bytes,
    fragments: Vec<SpannedFragment>,
}

/// Error returned by the parsing functions.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("template fragment does not contain valid UTF8: {0:?}")]
    NonUtf8TemplateFragment(Vec<u8>),
    #[error("unknown template fragment specifier '{0}'")]
    UnknownTemplateSpecifier(String),
}

/// A fragment with a span.
struct SpannedFragment {
    span: Range<usize>,
    kind: Fragment,
}

/// A parsed template fragment.
#[derive(Debug)]
pub enum Fragment {
    // TODO: one could avoid allocating those `String`s by storing `Byte`s
    // instead. However, this would add more UTF8 checks and/or unsafe blocks.
    // Not worth it for now.

    /// Inserts the public path of another asset. Example:
    /// `{{: path:bundle.js :}}`.
    Path(String),

    /// Includes another asset. Example: `{{: include:fonts.css :}}`.
    Include(String),

    /// Interpolates a runtime variable. Example: `{{: var:name :}}`.
    Var(String),
}

impl Fragment {
    fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let val = |s: &str| s[s.find(':').unwrap() + 1..].to_string();

        let s = std::str::from_utf8(bytes)
            .map_err(|_| Error::NonUtf8TemplateFragment(bytes.into()))?
            .trim();

        match () {
            () if s.starts_with("path:") => Ok(Self::Path(val(s))),
            () if s.starts_with("include:") => Ok(Self::Include(val(s))),
            () if s.starts_with("var:") => Ok(Self::Var(val(s))),

            _ => {
                let specifier = s[..s.find(':').unwrap_or(s.len())].to_string();
                Err(Error::UnknownTemplateSpecifier(specifier))
            }
        }
    }

    pub fn as_include(&self) -> Option<&str> {
        match self {
            Self::Include(p) => Some(p),
            _ => None,
        }
    }
}

impl Template {
    /// Parses the input byte string as template. Returns `Err` on parse error.
    pub fn parse(input: Bytes) -> Result<Self, Error> {
        let fragments = FragmentSpans::new(&input)
            .map(|span| {
                let kind = Fragment::parse(&input[span.clone()])?;
                Ok(SpannedFragment { span, kind })
            })
            .collect::<Result<_, _>>()?;

        Ok(Self {
            raw: input,
            fragments,
        })
    }

    /// Returns `Some(out)` if this template does not have any fragments at all.
    /// `out` is equal to the `input` that was passed to `parse`.
    pub fn into_already_rendered(self) -> Result<Bytes, Self> {
        if self.fragments.is_empty() {
            Ok(self.raw)
        } else {
            Err(self)
        }
    }

    /// Returns an iterator over all fragments.
    pub fn fragments(&self) -> impl Iterator<Item = &Fragment> {
        self.fragments.iter().map(|f| &f.kind)
    }

    /// Returns the raw input that was passed to `parse`.
    pub fn raw_input(&self) -> &Bytes {
        &self.raw
    }

    /// Renders the template using `replacer` to evaluate the fragments.
    ///
    /// # Replacing/evaluating fragments
    ///
    /// For each fragment in the `input` template, the `replacer` is called with
    /// the parsed fragment. For example, the template string `foo {{: bar :}}
    /// baz {{: config.data   :}}x` would lead to two calls to `replacer`, with
    /// the following strings as first parameter:
    ///
    /// - `bar`
    /// - `config.data`
    ///
    /// As you can see, excess whitespace is stripped before passing the string
    /// within the fragment.
    pub fn render<R, E>(self, mut replacer: R) -> Result<Bytes, E>
    where
        R: FnMut(Fragment, Appender) -> Result<(), E>,
    {
        if self.fragments.is_empty() {
            return Ok(self.raw);
        }

        let mut out = Vec::new();
        let mut last_fragment_end = 0;

        for fragment in self.fragments {
            // Add the part from the last fragment (or start) to the beginning
            // of this fragment.
            out.extend_from_slice(
                &self.raw[last_fragment_end..fragment.span.start - FRAGMENT_START.len()]
            );

            // Evaluate the fragment.
            replacer(fragment.kind, Appender(&mut out))?;

            last_fragment_end = fragment.span.end +  FRAGMENT_END.len();
        }

        // Add the stuff after the last fragment.
        out.extend_from_slice(&self.raw[last_fragment_end..]);

        Ok(out.into())
    }
}


/// An iterator over the spans of all template fragments in `input`, in order.
///
/// The iterator's item is the span (`Range<usize>`) of the fragment. The span
/// excludes the fragment start and end token, but includes potential excess
/// whitespace. Example:
///
/// ```text
/// input:    b"a{{: kk   :}}b"
/// indices:    0123456789012
/// ```
///
/// For that input, one span would be yielded by the iterator: `5..9`
///  (`input[5..9]` is `"kk  "`).
pub struct FragmentSpans<'a> {
    input: &'a [u8],
    idx: usize,
}

impl<'a> FragmentSpans<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            idx: 0,
        }
    }
}

impl Iterator for FragmentSpans<'_> {
    type Item = Range<usize>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.input.len() {
            return None;
        }

        while let Some(start_pos) = find(&self.input[self.idx..], FRAGMENT_START) {
            // We have a fragment candidate. Now we need to make sure that it's
            // actually a valid fragment.
            let end_pos = self.input[self.idx + start_pos..]
                .windows(FRAGMENT_END.len())
                .take(MAX_FRAGMENT_LEN - FRAGMENT_END.len() + 1)
                .take_while(|win| win[0] != b'\n')
                .position(|win| win == FRAGMENT_END);

            match end_pos {
                // We haven't found a matching end marker: ignore this start marker.
                None => {
                    self.idx += start_pos + FRAGMENT_START.len();
                }

                // This is a real fragment.
                Some(end_pos) => {
                    let start = self.idx + start_pos;
                    self.idx = start + end_pos + FRAGMENT_END.len();

                    return Some(start + FRAGMENT_START.len()..start + end_pos);
                }
            }
        }

        self.idx = self.input.len();
        None
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|win| win == needle)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_replacer(f: Fragment, mut appender: Appender) -> Result<(), ()> {
        match f {
            Fragment::Include(p) => {
                appender.append(b"i-");
                appender.append(p.to_uppercase().as_bytes());
            }
            Fragment::Path(p) => {
                appender.append(b"p-");
                appender.append(&p.bytes().rev().collect::<Vec<_>>())
            }
            Fragment::Var(k) => {
                appender.append(b"v-");
                appender.append(k.to_lowercase().as_bytes())
            }
        }

        Ok(())
    }

    fn render(input: &[u8]) -> Bytes {
        let template = Template::parse(Bytes::copy_from_slice(input)).expect("failed to parse");
        template.render(dummy_replacer).unwrap()
    }

    #[test]
    fn render_no_fragments() {
        let s = b"foo, bar, baz";
        let res = render(s);
        assert_eq!(res, s as &[_]);
    }

    #[test]
    fn render_simple_fragments() {
        assert_eq!(
            render(b"{{: include:banana :}}"),
            b"i-BANANA" as &[u8],
        );
        assert_eq!(
            render(b"foo {{: path:cat :}}baz"),
            b"foo p-tacbaz" as &[u8],
        );
        assert_eq!(
            render(b"foo {{: include:cat :}}baz{{: var:DOG :}}"),
            b"foo i-CATbazv-dog" as &[u8],
        );
    }

    #[test]
    fn render_ignored_fragments() {
        assert_eq!(
            render(b"x{{: a\nb :}}y"),
            b"x{{: a\nb :}}y" as &[u8],
        );
        assert_eq!(
            render(b"x{{: a\n {{: include:kiwi :}}y"),
            b"x{{: a\n i-KIWIy" as &[u8],
        );

        let long = b"foo {:: \
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            yo ::} bar\
        " as &[u8];
        assert_eq!(render(long), long);
    }
}
