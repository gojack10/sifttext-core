//! Complete-file costs. Disk metadata is an untrusted projection, not a count oracle.
use crate::{LIMIT, Result, require, sha256};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub const ENCODING: &str = "o200k_base";
// Change this identity whenever the counting implementation/boundary changes.
pub const TOKENIZER_VERSION: &str = "tiktoken-rs/0.6.0;ordinary;complete-utf8/v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub tokens: usize,
    pub hash: String,
    pub encoding: String,
    pub tokenizer_version: String,
}

#[derive(Debug, Serialize)]
pub struct Refresh {
    pub status: &'static str,
    #[serde(flatten)]
    pub measurement: Measurement,
    /// Only true for computation retained by this live Refresher, never a disk claim.
    pub reused: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseBeforeAccept {
    pub ready: PathBuf,
    pub release: PathBuf,
}

impl PauseBeforeAccept {
    fn wait(&self, source_dir: &Path) -> Result<()> {
        for path in [&self.ready, &self.release] {
            let parent = fs::canonicalize(path.parent().ok_or("missing barrier parent")?)?;
            require(
                !parent.starts_with(source_dir),
                "barrier must be outside source directory",
            )?;
        }
        require(self.ready != self.release, "barrier paths must differ")?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.ready)?
            .write_all(b"computed\n")?;
        let start = Instant::now();
        while !self.release.try_exists()? {
            require(start.elapsed() < Duration::from_secs(10), "barrier expired")?;
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

fn identity(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}

fn same_version(a: &Metadata, b: &Metadata) -> bool {
    identity(a, b)
        && a.len() == b.len()
        && a.nlink() == b.nlink()
        && a.mtime() == b.mtime()
        && a.mtime_nsec() == b.mtime_nsec()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}

fn regular(file: &File, path: &Path) -> Result<Metadata> {
    let meta = file.metadata()?;
    let named = fs::symlink_metadata(path)?;
    require(
        meta.is_file() && meta.nlink() == 1 && named.is_file() && identity(&meta, &named),
        "unsafe or replaced native file",
    )?;
    Ok(meta)
}

fn open_read(path: &Path) -> Result<File> {
    // Nonblocking also prevents a raced-in FIFO from hanging an inspection request.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    regular(&file, path)?;
    Ok(file)
}

struct Capture {
    bytes: Vec<u8>,
    meta: Metadata,
}

fn capture(path: &Path) -> Result<Capture> {
    let file = open_read(path)?;
    let meta = regular(&file, path)?;
    require(meta.len() <= LIMIT as u64, "oversized node file")?;
    let mut bytes = Vec::new();
    (&file).take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    require(
        bytes.len() <= LIMIT && same_version(&meta, &regular(&file, path)?),
        "source changed during capture",
    )?;
    std::str::from_utf8(&bytes)?;
    Ok(Capture { bytes, meta })
}

fn sidecar(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
        Ok(meta) => {
            require(meta.is_file() && meta.nlink() == 1, "unsafe metadata entry")?;
            let file = open_read(path)?;
            let mut bytes = Vec::new();
            (&file).take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
            // Oversized/corrupt metadata is never parsed or trusted; replace it.
            Ok(Some(bytes))
        }
    }
}

/// Only canonical node-local outputs are disposable; a matching basename elsewhere
/// is still protected. This classification never asserts that a disk count is current.
pub(crate) fn derived(relative: &Path) -> bool {
    let parts: Vec<_> = relative.components().collect();
    parts.len() == 3
        && parts[0].as_os_str() == "nodes"
        && parts[1].as_os_str().to_str().is_some_and(crate::uuid)
        && ["node.meta.json", "node.meta.lock"]
            .iter()
            .any(|name| parts[2].as_os_str() == *name)
}

pub(crate) fn safe_derived(path: &Path) -> Result<bool> {
    let file = open_read(path)?;
    let meta = regular(&file, path)?;
    // The persistent serialization inode has no payload. Do not hide arbitrary
    // protected bytes under the lock name, or remove it and split serialization.
    if path.file_name().is_some_and(|n| n == "node.meta.lock") {
        require(meta.len() == 0, "nonempty token metadata lock")?;
        return Ok(true);
    }
    let mut bytes = Vec::new();
    (&file).take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    require(
        same_version(&meta, &regular(&file, path)?),
        "metadata changed during capture",
    )?;
    // Only the maintained output schema is regenerable. Malformed/foreign bytes
    // remain protected inventory, including status normalization's race negatives.
    // Shape is NOT proof of a current or honest count: Refresher alone computes it.
    Ok(
        serde_json::from_slice::<Measurement>(&bytes).is_ok_and(|m| {
            m.encoding == ENCODING
                && m.tokenizer_version == TOKENIZER_VERSION
                && m.hash.len() == 64
                && m.hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }),
    )
}

/// One bounded, trusted computation cache per consumer. No global mutable path identity.
#[derive(Default)]
pub struct Refresher {
    // ponytail: one-entry cache, <=1 MiB source; add bounded LRU only if workload measures need it.
    cached: Option<(Vec<u8>, Measurement)>,
}

impl Refresher {
    pub fn refresh(&mut self, path: &Path, pause: Option<&PauseBeforeAccept>) -> Result<Refresh> {
        require(
            path.file_name().is_some_and(|n| n == "node.md"),
            "expected node.md",
        )?;
        let parent = fs::canonicalize(path.parent().ok_or("missing source parent")?)?;
        let parent_meta = fs::symlink_metadata(&parent)?;
        require(parent_meta.is_dir(), "unsafe source directory")?;
        let source = parent.join("node.md");
        let metadata = parent.join("node.meta.json");
        let lock_path = parent.join("node.meta.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&lock_path)?;
        require(
            regular(&lock, &lock_path)?.len() == 0,
            "nonempty token metadata lock",
        )?;
        let start = Instant::now();
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) => {
                    require(
                        start.elapsed() < Duration::from_secs(10),
                        "refresh lock expired",
                    )?;
                    thread::sleep(Duration::from_millis(5));
                }
                Err(e) => return Err(e.into()),
            }
        }
        // Keep the lock inode alive: removing/recreating it would split serialization.
        require(
            regular(&lock, &lock_path)?.len() == 0,
            "nonempty token metadata lock",
        )?;
        let captured = capture(&source)?;
        let reused = self
            .cached
            .as_ref()
            .is_some_and(|(b, _)| *b == captured.bytes);
        let measurement = if reused {
            self.cached.as_ref().unwrap().1.clone()
        } else {
            let bpe = tiktoken_rs::o200k_base_singleton();
            let tokens = bpe
                .lock()
                .encode_ordinary(std::str::from_utf8(&captured.bytes)?)
                .len();
            Measurement {
                tokens,
                hash: sha256(&captured.bytes),
                encoding: ENCODING.into(),
                tokenizer_version: TOKENIZER_VERSION.into(),
            }
        };
        let mut bytes = serde_json::to_vec(&measurement)?;
        bytes.push(b'\n');
        let previous = sidecar(&metadata)?;
        let mut temp = tempfile::NamedTempFile::new_in(&parent)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        if let Some(pause) = pause {
            pause.wait(&parent)?;
        }
        let current = || -> Result<bool> {
            require(
                fs::canonicalize(path.parent().ok_or("missing parent")?)? == parent
                    && identity(&parent_meta, &fs::symlink_metadata(&parent)?),
                "source directory replaced",
            )?;
            require(
                regular(&lock, &lock_path)?.len() == 0,
                "nonempty token metadata lock",
            )?;
            let now = capture(&source)?;
            Ok(same_version(&captured.meta, &now.meta) && now.bytes == captured.bytes)
        };
        if !current()? {
            return Ok(Refresh {
                status: "stale",
                measurement,
                reused,
            });
        }
        // Recheck unsafe destination shapes after the test barrier too. Never follow them.
        let latest = sidecar(&metadata)?;
        if previous.as_deref() != Some(&bytes) || latest.as_deref() != Some(&bytes) {
            temp.persist(&metadata)?;
            File::open(&parent)?.sync_all()?;
        }
        // Writers do not share this lock. Validate after publication as well; consumers
        // must compare hashes at read time, not assume a persisted sidecar stays current.
        if !current()? {
            return Ok(Refresh {
                status: "stale",
                measurement,
                reused,
            });
        }
        self.cached = Some((captured.bytes, measurement.clone()));
        Ok(Refresh {
            status: "current",
            measurement,
            reused,
        })
    }
}
