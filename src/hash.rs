use bytes::Bytes;
use reinda_core::AssetDef;


#[cfg(not(feature = "hash"))]
pub(crate) fn hashed_path_of(_: &AssetDef, _: &Bytes) -> String {
    // The `assets!` macro should error if an asset specifies `hash`.
    unreachable!("`hash_path_of` called, but Cargo-feature 'hash' is disabled.");
}


#[cfg(feature = "hash")]
pub(crate) fn hashed_path_of(def: &AssetDef, content: &Bytes) -> String {
    use std::path::Path;
    use sha2::{Digest, Sha256};

    /// How many bytes of the 32 byte (256 bit) hash are used and encoded in the
    /// filename.
    const HASH_BYTES_IN_FILENAME: usize = 9;


    let (first, second) = def.hash.expect("called `hashed_path_of`, but `def.hash` is None");

    let mut out = String::new();

    // First add the parent directory, if any.
    if let Some(parent) = Path::new(def.path).parent() {
        if parent.iter().count() > 0 {
            out.push_str(parent.to_str().unwrap());
            out.push('/');
        }
    }

    // Next, the first part of the filename.
    out.push_str(first);

    // Calc and then base64 encode the hash.
    let hash = Sha256::digest(&content);
    base64::encode_config_buf(
        &hash.as_slice()[..HASH_BYTES_IN_FILENAME],
        base64::URL_SAFE_NO_PAD,
        &mut out,
    );

    // Finally the second part of the filename
    out.push_str(second);

    out
}
