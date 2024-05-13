use aho_corasick::AhoCorasick;


pub fn replace_many<N, R>(src: &[u8], replacements: &[(N, R)]) -> Vec<u8>
where
    N: AsRef<[u8]>,
    R: AsRef<[u8]>,
{
    let needles = replacements.iter().map(|(needle, _)| needle);
    let replacer = AhoCorasick::new(needles).unwrap();
    let mut out = Vec::with_capacity(src.len());
    replacer.replace_all_with_bytes(&src, &mut out, |m, _, out| {
        out.extend_from_slice(replacements[m.pattern().as_usize()].1.as_ref());
        true
    });
    out.into()
}

pub fn replace_many_with<N, T, R>(src: &[u8], needles: N, mut f: R) -> Vec<u8>
where
    N: IntoIterator<Item = T>,
    T: AsRef<[u8]>,
    R: FnMut(usize, &[u8], &mut Vec<u8>),
{
    let replacer = AhoCorasick::new(needles).unwrap();
    let mut out = Vec::with_capacity(src.len());
    replacer.replace_all_with_bytes(&src, &mut out, |m, find, out| {
        f(m.pattern().as_usize(), find, out);
        true
    });
    out.into()
}
