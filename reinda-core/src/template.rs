use std::{borrow::Cow, ops::Range};



const FRAGMENT_START: &[u8] = b"{{: ";
const FRAGMENT_END: &[u8] = b" :}}";

/// Fragments longer than this are ignored. This is just to protect against
/// random fragment start/end markers in large generated files.
const MAX_FRAGMENT_LEN: usize = 256;


/// A byte string that you can append to. Used for `render`.
pub struct Appender<'a>(&'a mut Vec<u8>);

impl Appender<'_> {
    pub fn append(&mut self, s: &[u8]) {
        self.0.extend_from_slice(s);
    }
}

/// Renders the template `input`, using `replacer` to evaluate the fragments.
///
/// # Template syntax
///
/// Our template syntax is super simple and is really just a glorified
/// search-and-replace. The input is checked for "fragments" which have the
/// syntax `{{: foo :}}`. The start token is actually `{{: ` (note the
/// whitespace!). So `{{:foo:}}` is not recognized as token.
///
/// There are two additional constraints: the fragment must not contain a
/// newline and must be shorter than [`MAX_FRAGMENT_LEN`]. If these conditions
/// are not met, the fragment start token is ignored.
///
///
/// # Replacing/evaluating fragments
///
/// For each fragment in the `input` template, the `replacer` is called with
/// the string within the fragment. For example, the template string
/// `foo {{: bar :}} baz {{: config.data   :}}x` would lead to two calls to
/// `replacer`, with the following strings as first parameter:
///
/// - `bar`
/// - `config.data`
///
/// As you can see, excess whitespace is stripped before passing the string
/// within the fragment.
pub fn render<R, E>(input: &[u8], mut replacer: R) -> Result<Cow<'_, [u8]>, E>
where
    R: FnMut(&[u8], Appender) -> Result<(), E>,
{
    let mut out = Vec::new();
    let mut last_fragment_end = 0;

    for span in FragmentSpans::new(input) {
        out.extend_from_slice(&input[last_fragment_end..span.start - FRAGMENT_START.len()]);
        replacer(&input[span.clone()], Appender(&mut out))?;
        last_fragment_end = span.end +  FRAGMENT_END.len();
    }

    if last_fragment_end != 0 {
        out.extend_from_slice(&input[last_fragment_end..]);
        Ok(Cow::Owned(out))
    } else {
        Ok(Cow::Borrowed(input))
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

    #[test]
    fn render_no_fragments() {
        let s = b"foo, bar, baz" as &[u8];
        let res = render(s, |_, _| {});
        assert!(matches!(res, Cow::Borrowed(_)));
        assert_eq!(res, s);
    }

    #[test]
    fn render_simple_fragments() {
        let append_uppercase = |k: &[u8], mut a: Appender| a.append(&k.to_ascii_uppercase());

        assert_eq!(
            render(b"{{: banana :}}", append_uppercase),
            b"BANANA" as &[u8],
        );
        assert_eq!(
            render(b"foo {{: cat :}}baz", append_uppercase),
            b"foo CATbaz" as &[u8],
        );
        assert_eq!(
            render(b"foo {{: cat :}}baz{{: dog :}}", append_uppercase),
            b"foo CATbazDOG" as &[u8],
        );
    }

    #[test]
    fn render_ignored_fragments() {
        let append_uppercase = |k: &[u8], mut a: Appender| a.append(&k.to_ascii_uppercase());

        // assert_eq!(
        //     render(b"x{{: a\nb :}}y", append_uppercase),
        //     b"x{{: a\nb :}}y" as &[u8],
        // );
        assert_eq!(
            render(b"x{{: a\n {{: kiwi :}}y", append_uppercase),
            b"x{{: a\n KIWIy" as &[u8],
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
        assert_eq!(render(long, append_uppercase), long);
    }
}
