use bytes::Bytes;

use crate::PathHash;


#[derive(Debug)]
pub(crate) struct PathMap<'a> {
    #[cfg(feature = "hash")]
    map: ahash::HashMap<&'a str, String>,

    #[cfg(not(feature = "hash"))]
    map: std::marker::PhantomData<&'a ()>,
}

impl<'a> PathMap<'a> {
    pub(crate) fn new() -> Self {
        #[cfg(feature = "hash")]
        { Self { map: ahash::HashMap::default() } }

        #[cfg(not(feature = "hash"))]
        { Self { map: std::marker::PhantomData } }
    }

    pub(crate) fn get(&self, path: &str) -> Option<&str> {
        #[cfg(feature = "hash")]
        { self.map.get(path).map(|s| &**s) }

        #[cfg(not(feature = "hash"))]
        {
            let _path = path;
            None
        }
    }
}

#[cfg(not(feature = "hash"))]
pub(crate) fn path_of<'a>(
    _: PathHash<'_>,
    path: &'a str,
    _: &Bytes,
    _: &mut PathMap<'a>,
) -> String {
    path.to_owned()
}


#[cfg(feature = "hash")]
pub(crate) fn path_of<'a>(
    hash: PathHash<'_>,
    path: &'a str,
    content: &Bytes,
    map: &mut PathMap<'a>,
) -> String {
    use sha2::{Digest, Sha256};
    use base64::Engine;


    /// How many bytes of the 32 byte (256 bit) hash are used and encoded in the
    /// filename. We use a multiple of 9, as base64 encodes 3 bytes with 4
    /// chars. With a multiple of 3 input bytes, we do not waste base64 chars.
    const HASH_BYTES_IN_FILENAME: usize = 9;


    let (first_part, hash_prefix, second_part) = match hash {
        PathHash::None => return path.to_owned(),
        PathHash::Auto => {
            let last_seg_start = path.rfind('/').map(|p| p + 1).unwrap_or(0);
            let (pos, hash_prefix) = match path[last_seg_start..].find('.') {
                Some(pos) => (last_seg_start + pos, '.'),
                None => (path.len(), '-'),
            };

            (&path[..pos], Some(hash_prefix), &path[pos..])
        },
        PathHash::InBetween { prefix, suffix } => (prefix, None, suffix),
    };

    // Calculate hash
    let hash = Sha256::digest(&content);

    // Concat everything including the base64 encoded hash
    let mut out = first_part.to_owned();
    out.extend(hash_prefix);
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode_string(&hash.as_slice()[..HASH_BYTES_IN_FILENAME], &mut out);
    out.push_str(second_part);

    // Add entry to path map
    map.map.insert(path, out.clone());

    out
}
