use std::borrow::Cow;



const FRAGMENT_START: &str = "{{: ";
const FRAGMENT_END: &str = " :}}";

/// Fragments longer than this are ignored. This is just to protect against
/// random fragment start/end markers in large generated files.
const MAX_FRAGMENT_LEN: usize = 256;


/// A string that you can append to. Used for `render`.
pub struct Appender<'a>(&'a mut String);

impl Appender<'_> {
    pub fn append(&mut self, s: &str) {
        self.0.push_str(s);
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
pub fn render<R>(input: &str, mut replacer: R) -> Cow<'_, str>
where
    R: FnMut(&str, Appender)
{
    let mut out = String::new();
    let mut last_fragment_end = 0;

    let mut idx = 0;
    while let Some(pos) = input[idx..].find(FRAGMENT_START) {
        // We have a fragment candidate. Now we need to make sure that it's
        // actually a valid fragment.
        let end_pos = input[idx + pos..]
            .lines()
            .next()
            .and_then(|l| {
                let end = std::cmp::min(l.len(), MAX_FRAGMENT_LEN);
                l[..end].find(FRAGMENT_END)
            });

        match end_pos {
            // We haven't found a matching end marker: ignore this start marker.
            None => idx += pos + FRAGMENT_START.len(),

            // This is a real fragment and we will now substitute.
            Some(end_pos) => {
                let start = idx + pos;
                out.push_str(&input[last_fragment_end..start]);

                let inner = &input[start + FRAGMENT_START.len()..start + end_pos];
                replacer(inner.trim(), Appender(&mut out));

                last_fragment_end = start + end_pos + FRAGMENT_END.len();
                idx = last_fragment_end;
            }
        }
    }

    if last_fragment_end != 0 {
        out.push_str(&input[last_fragment_end..]);
        Cow::Owned(out)
    } else {
        Cow::Borrowed(input)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_no_fragments() {
        let s = "foo, bar, baz";
        let res = render(s, |_, _| {});
        assert!(matches!(res, Cow::Borrowed(_)));
        assert_eq!(res, s);
    }

    #[test]
    fn render_simple_fragments() {
        let append_uppercase = |k: &str, mut a: Appender| a.append(&k.to_uppercase());

        assert_eq!(
            render("{{: banana :}}", append_uppercase),
            "BANANA",
        );
        assert_eq!(
            render("foo {{: cat :}}baz", append_uppercase),
            "foo CATbaz",
        );
        assert_eq!(
            render("foo {{: cat :}}baz{{: dog :}}", append_uppercase),
            "foo CATbazDOG",
        );
    }

    #[test]
    fn render_ignored_fragments() {
        let append_uppercase = |k: &str, mut a: Appender| a.append(&k.to_uppercase());

        assert_eq!(
            render("x{{: a\nb :}}y", append_uppercase),
            "x{{: a\nb :}}y",
        );
        assert_eq!(
            render("x{{: a\n {{: kiwi :}}y", append_uppercase),
            "x{{: a\n KIWIy",
        );

        let long = "foo {:: \
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrstuvwxy\
            yo ::} bar\
        ";
        assert_eq!(render(long, append_uppercase), long);
    }
}
