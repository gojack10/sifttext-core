//! Shared opaque identity of the authored logical inventory (not freshness authority).
use crate::{Result, require};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) const ALGORITHM: &str = "authored-merkle-v2";

pub(crate) fn start() -> Sha256 {
    let mut hash = Sha256::new();
    hash.update(b"sifttext-authored-merkle-v2\0");
    hash
}

pub(crate) enum Part<'a> {
    Directory,
    File { len: u64, hash: &'a [u8; 32] },
    Link { len: u64, hash: &'a [u8; 32] },
}

pub(crate) fn add(hash: &mut Sha256, path: &Path, part: Part<'_>) -> Result<()> {
    let path = path.to_str().ok_or("non-UTF-8 authored path")?;
    require(!path.is_empty(), "empty authored path")?;
    hash.update((path.len() as u64).to_le_bytes());
    hash.update(path.as_bytes());
    match part {
        Part::Directory => hash.update([0]),
        Part::File { len, hash: digest } | Part::Link { len, hash: digest } => {
            hash.update([if matches!(part, Part::File { .. }) {
                1
            } else {
                2
            }]);
            hash.update(len.to_le_bytes());
            hash.update(digest);
        }
    }
    Ok(())
}
