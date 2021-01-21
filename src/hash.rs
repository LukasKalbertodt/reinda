use std::path::Path;

use bytes::Bytes;
use sha2::{Digest, Sha256};


/// How many bytes of the 32 byte (256 bit) hash are used and encoded in the
/// filename.
const HASH_BYTES_IN_FILENAME: usize = 6;

pub(crate) fn hashed_path_of(path: &str, content: &Bytes) -> String {
    let mut out = String::new();

    // Calc and then base64 encode the hash.
    let hash = Sha256::digest(&content);
    base64::encode_config_buf(
        &hash.as_slice()[..HASH_BYTES_IN_FILENAME],
        base64::URL_SAFE_NO_PAD,
        &mut out,
    );

    out.push('-');

    let filename = Path::new(path).file_name()
        .expect("asset path has no filename")
        .to_str()
        .unwrap();
    out.push_str(filename);

    out
}
