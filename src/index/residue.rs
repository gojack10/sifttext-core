//! Narrow cached-byte ownership for source-less native outline owners.
use crate::{Result, require};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, os::unix::fs::{MetadataExt, OpenOptionsExt}, path::Path};

const NAMES: [&str; 4] = ["outline.lean", "outline.lean.meta.json", "outline.full.lean", "outline.full.lean.meta.json"];

fn directory(path: &Path) -> Result<(u64, u64)> {
    let m = fs::symlink_metadata(path)?;
    require(m.is_dir(), "unsafe residue directory")?;
    Ok((m.dev(), m.ino()))
}
fn known(connection: &Connection, id: &str) -> Result<()> {
    let table: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='outline_cache')", [], |r| r.get(0))?;
    require(table, "preserved source-less node: no native outline_cache ownership proof; restore authored files or migrate the legacy cache")?;
    let owner: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM outline_cache WHERE node_id=?1 AND path IN (?2,?3,?4,?5))", params![id,
        format!("nodes/{id}/{}", NAMES[0]), format!("nodes/{id}/{}", NAMES[1]),
        format!("nodes/{id}/{}", NAMES[2]), format!("nodes/{id}/{}", NAMES[3])], |r| r.get(0))?;
    require(owner, "preserved source-less node: no cached owner proof")
}

// Paths are constructed from validated source enumeration, never selected by cache text.
fn file(connection: &Connection, root: &Path, id: &str, name: &str) -> Result<()> {
    let relative = format!("nodes/{id}/{name}");
    let proof: Option<(String, i64)> = connection.query_row("SELECT bytes_hash,size FROM outline_cache WHERE path=?1 AND node_id=?2", params![relative, id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    let (expected, size) = proof.ok_or("preserved residue file: missing exact cached-byte proof")?;
    require(expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "invalid residue hash proof")?;
    let size = u64::try_from(size)?;
    let path = root.join(&relative);
    let before = fs::symlink_metadata(&path)?;
    require(before.is_file() && before.nlink() == 1 && before.len() == size, "preserved residue file: unsafe or changed bytes")?;
    let signature = |m: &fs::Metadata| (m.dev(), m.ino(), m.len(), m.mtime(), m.mtime_nsec(), m.ctime(), m.ctime_nsec(), m.nlink());
    let mut input = fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&path)?;
    require(signature(&input.metadata()?) == signature(&before), "changed residue identity")?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 32 * 1024];
    let mut read = 0u64;
    loop {
        let n = input.read(&mut buffer)?;
        if n == 0 { break }
        read = read.checked_add(n as u64).ok_or("residue size overflow")?;
        require(read <= size, "changed residue bytes")?;
        hash.update(&buffer[..n]);
    }
    require(read == size && format!("{:x}", hash.finalize()) == expected
        && signature(&input.metadata()?) == signature(&before)
        && signature(&fs::symlink_metadata(path)?) == signature(&before), "preserved residue file: cached bytes no longer match")
}

/// No authored/unknown entry is permitted, including symlinks or node metadata.
/// Previous processor outputs remain owned bytes; processor currency is irrelevant.
pub(super) fn verify(connection: &Connection, root: &Path, id: &str) -> Result<()> {
    require(crate::uuid(id), "invalid residue owner")?;
    let root_id = directory(root)?;
    let nodes_id = directory(&root.join("nodes"))?;
    let owner = root.join("nodes").join(id);
    let owner_id = directory(&owner)?;
    known(connection, id)?;
    let before = fs::symlink_metadata(&owner)?;
    for entry in fs::read_dir(&owner)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or("non-UTF-8 residue entry")?;
        require(NAMES.contains(&name), "preserved source-less node: authored or unknown resources remain")?;
        file(connection, root, id, name)?;
    }
    let after = fs::symlink_metadata(&owner)?;
    require(directory(root)? == root_id && directory(&root.join("nodes"))? == nodes_id
        && directory(&owner)? == owner_id
        && (before.mtime(), before.mtime_nsec(), before.ctime(), before.ctime_nsec())
            == (after.mtime(), after.mtime_nsec(), after.ctime(), after.ctime_nsec()), "changed residue parent identity")
}

/// Re-prove remaining namespace and bytes before each unlink. This is not an
/// adversarial atomic-unlink primitive: a final native path race remains possible.
pub(crate) fn retire(connection: &Connection, root: &Path, id: &str) -> Result<()> {
    verify(connection, root, id)?;
    let owner = root.join("nodes").join(id);
    let root_id = directory(root)?;
    let nodes_id = directory(&root.join("nodes"))?;
    let owner_id = directory(&owner)?;
    for name in NAMES {
        verify(connection, root, id)?;
        require(directory(root)? == root_id && directory(&root.join("nodes"))? == nodes_id
            && directory(&owner)? == owner_id, "changed residue parent identity")?;
        let path = owner.join(name);
        match fs::symlink_metadata(&path) {
            Ok(_) => { file(connection, root, id, name)?; fs::remove_file(path)?; }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    verify(connection, root, id)?;
    require(directory(&owner)? == owner_id, "changed residue owner identity")?;
    fs::remove_dir(owner)?;
    Ok(())
}
