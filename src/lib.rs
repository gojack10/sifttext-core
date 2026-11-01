use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

// Standalone observers and reusable modules share this public crate identity.
extern crate self as sifttext_file_graph;

/// Return freed phase buffers to the OS before spawning another working set.
pub(crate) fn release_unused_memory() {
    #[cfg(target_os = "macos")]
    unsafe {
        unsafe extern "C" {
            fn malloc_zone_pressure_relief(zone: *mut libc::c_void, goal: usize) -> usize;
        }
        malloc_zone_pressure_relief(std::ptr::null_mut(), 0);
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe { libc::malloc_trim(0); }
}

// Per-command policy: fence only knobs that can transform authored bytes or run
// repository-supplied programs. Git keeps its own caches, stat handling and
// index threading.
pub(crate) const GIT_CONFIG_FENCES: [&str; 10] = [
    "-c", "core.autocrlf=false",
    "-c", "core.attributesFile=/dev/null",
    "-c", "core.filemode=true",
    "-c", "core.symlinks=true",
    "-c", "diff.external=",
];

/// Directory holding the native helper binaries built beside this test binary.
/// `CARGO_MANIFEST_DIR/target/debug` is the wrong answer whenever the build
/// used an external `CARGO_TARGET_DIR`, which is the project's preferred
/// location for generated targets.
#[cfg(test)]
pub(crate) fn native_bin_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(profile) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|deps| deps.to_path_buf()))
        .and_then(|deps| deps.parent().map(|profile| profile.to_path_buf()))
    {
        return profile;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug")
}

/// `cargo test --lib` does not build the helper binaries, so name the missing
/// prerequisite instead of letting a later spawn surface a bare NotFound.
#[cfg(test)]
pub(crate) fn native_bin(name: &str) -> PathBuf {
    let bin = native_bin_dir().join(name);
    assert!(
        bin.is_file(),
        "native test helper {name} is missing at {}; run `sh scripts/build.sh` in this checkout before `cargo test --lib`, or set SIFTTEXT_TEST_NATIVE_BIN_DIR to the directory that holds it",
        bin.display()
    );
    bin
}

pub(crate) mod patch_build;

#[cfg(test)]
#[path = "../tests/successor/candidate_refusals.rs"]
mod candidate_refusals_tests;
pub mod account;
pub mod candidate_validation;
#[cfg(test)]
#[path = "../tests/successor/candidate_validation.rs"]
mod candidate_validation_tests;
#[cfg(test)]
#[path = "../tests/successor/capture_residue.rs"]
mod capture_residue_tests;
#[path = "vendor/leviathan/card.rs"]
pub mod card;
pub mod checked_publication;
mod checked_publication_attribution;
#[cfg(test)]
#[path = "../tests/successor/checked_publication.rs"]
mod checked_publication_tests;
mod coherent_capture;
pub(crate) mod git_snapshot;
pub mod hosted_checks;
pub(crate) mod source_version;
#[cfg(test)]
#[path = "../tests/successor/coherent_capture.rs"]
mod coherent_capture_tests;
#[cfg(test)]
#[path = "../tests/successor/incremental_outlines.rs"]
mod incremental_outlines_tests;
pub mod index;
pub mod lean_render;
#[path = "vendor/leviathan/query_text.rs"]
pub mod leviathan_query;
#[path = "vendor/leviathan/search.rs"]
pub mod leviathan_search;
pub mod local;
#[cfg(test)]
#[path = "../tests/successor/local_onramp.rs"]
mod local_onramp_tests;
pub mod native_kb;
pub mod neighborhoods;
pub mod outline_artifacts;
#[cfg(test)]
#[path = "../tests/successor/outline_residue.rs"]
mod outline_residue_tests;
pub mod outline_rows;
pub mod outlines;
pub mod ordering;
pub mod tree_render;
pub mod tree_selection;
#[cfg(test)]
#[path = "../tests/successor/prepared_publication.rs"]
mod prepared_publication_tests;
#[cfg(test)]
#[path = "../tests/successor/publication_graph_transition.rs"]
mod publication_graph_transition_tests;
#[cfg(test)]
#[path = "../tests/successor/session_attachments.rs"]
mod session_attachment_tests;
#[cfg(test)]
#[path = "../tests/successor/session_backup.rs"]
mod session_backup_tests;
#[cfg(test)]
#[path = "../tests/successor/session_capture.rs"]
mod session_capture_tests;
#[cfg(test)]
#[path = "../tests/successor/session_retention_config.rs"]
mod session_retention_config_tests;
mod source_error;
#[cfg(test)]
#[path = "../tests/successor/structural_diagnostics.rs"]
mod structural_diagnostics_tests;

#[cfg(test)]
#[path = "../tests/successor/outline_scaling.rs"]
mod outline_scaling_tests;
#[cfg(test)]
#[path = "../tests/successor/outlines.rs"]
mod outlines_tests;
#[path = "vendor/leviathan/relevance.rs"]
pub mod relevance;
pub mod search;
pub mod scratch;
pub mod search_document;
#[path = "vendor/leviathan/snippet.rs"]
pub mod snippet;

#[cfg(test)]
#[path = "../tests/successor/search.rs"]
mod search_tests;

#[cfg(test)]
#[path = "../tests/successor/incremental_retrieval.rs"]
mod incremental_retrieval_tests;
#[cfg(test)]
#[path = "../tests/successor/index.rs"]
mod index_tests;
#[cfg(test)]
#[path = "../tests/successor/prepared_index_reuse.rs"]
mod prepared_index_reuse_tests;
#[cfg(test)]
#[path = "../tests/successor/retrieval_contention.rs"]
mod retrieval_contention_tests;
pub mod status;
pub mod token_count;
pub mod token_metadata;
pub mod updates;

#[cfg(test)]
#[path = "../tests/successor/token_metadata.rs"]
mod token_metadata_tests;

#[cfg(test)]
#[path = "../tests/successor/generated_metadata.rs"]
mod generated_metadata_tests;

#[cfg(test)]
#[path = "../tests/successor/status_gate.rs"]
mod status_gate_tests;
#[cfg(test)]
#[path = "../tests/successor/status.rs"]
mod status_tests;

#[cfg(test)]
#[path = "../tests/successor/neighborhoods.rs"]
mod neighborhood_tests;

#[cfg(test)]
#[path = "../tests/successor/cli_outcomes.rs"]
mod cli_outcomes_tests;

#[cfg(test)]
#[path = "../tests/successor/retrieval_refusals.rs"]
mod retrieval_refusals_tests;

#[cfg(test)]
#[path = "../tests/successor/wire_encoding.rs"]
mod wire_encoding_tests;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const LIMIT: usize = 1_048_576;
const PROCESS_DEADLINE: Duration = Duration::from_secs(10);
const MAX_POLICY_DEADLINE: Duration = Duration::from_secs(300);
// Frozen Runtime.bend cannot see a suffix beyond this read boundary.
const BACKEND_INPUT_BYTES: usize = 1_048_576;

#[cfg(test)]
#[path = "../tests/successor/process_framing.rs"]
mod process_framing_tests;

#[cfg(test)]
#[path = "../tests/successor/policy_deadline.rs"]
mod policy_deadline_tests;

#[cfg(test)]
#[path = "../tests/successor/store_caps.rs"]
mod store_caps_tests;

fn reserved<T>(count: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(count)?;
    Ok(values)
}

fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}

pub fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
            }
        })
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub target: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Authored {
    pub id: String,
    pub name: String,
    pub micro_summary: String,
    pub content: String,
    pub see_also: Vec<Reference>,
}

// A deliberately small front matter grammar: one key and JSON value per line.
// Body and unknown metadata remain opaque bytes.
struct Markdown<'a> {
    authored: Authored,
    fields: BTreeMap<&'a str, &'a str>,
}

fn markdown(text: &str) -> Result<Markdown<'_>> {
    #[cfg(test)]
    coherent_capture::count(|c| c.parses += 1);
    let mut lines = text.split_inclusive('\n');
    let first = lines.next().ok_or("missing Markdown metadata")?;
    require(
        first == "---\n" || first == "---\r\n",
        "missing Markdown opening fence",
    )?;
    let mut offset = first.len();
    let mut fields = BTreeMap::new();
    let mut body = None;
    for raw in lines {
        let line = raw
            .strip_suffix('\n')
            .ok_or("unterminated Markdown metadata")?;
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line == "---" {
            body = Some(&text[offset + raw.len()..]);
            break;
        }
        if !line.trim().is_empty() {
            let (key, value) = line.split_once(':').ok_or("malformed metadata line")?;
            let key = key.trim_matches([' ', '\t']);
            require(
                !key.is_empty()
                    && key.bytes().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
                    }),
                "malformed metadata key",
            )?;
            require(
                !["parent", "parents", "children"].contains(&key),
                "second authored hierarchy source",
            )?;
            let trimmed = value.trim_matches([' ', '\t']);
            // Validate even unknown metadata, but never reserialize it.
            let known = [
                "id",
                "name",
                "micro_summary",
                "see_also",
                "status",
                "scope",
                "warnings",
                "blocker",
                "deferred_reason",
                "discard_reason",
            ]
            .contains(&key)
            .then_some(key);
            source_error::context(
                serde_json::from_str::<serde_json::Value>(trimmed)
                    .map(|_| ())
                    .map_err(Into::into),
                Path::new("."),
                known,
            )?;
            source_error::context(
                require(
                    fields.insert(key, trimmed).is_none(),
                    "duplicate metadata key",
                ),
                Path::new("."),
                known,
            )?;
        }
        offset += raw.len();
    }
    fn field<'a, T: serde::Deserialize<'a>>(
        fields: &BTreeMap<&str, &'a str>,
        key: &str,
    ) -> Result<T> {
        source_error::context(
            (|| {
                Ok(serde_json::from_str(
                    fields
                        .get(key)
                        .copied()
                        .ok_or("missing required metadata")?,
                )?)
            })(),
            Path::new("."),
            Some(key),
        )
    }
    let authored = Authored {
        id: field(&fields, "id")?,
        name: field(&fields, "name")?,
        micro_summary: field(&fields, "micro_summary")?,
        see_also: field(&fields, "see_also")?,
        content: body.ok_or("missing Markdown closing fence")?.to_owned(),
    };
    Ok(Markdown {
        authored,
        fields,
    })
}

pub fn parse_markdown(text: &str) -> Result<Authored> {
    Ok(markdown(text)?.authored)
}

#[derive(Clone, Debug)]
pub enum Entry {
    File(Vec<u8>),
    /// Invocation-local captured bytes, never an authority for later source reads.
    FileRef { source: PathBuf, len: u64, digest: [u8; 32] },
    Link(PathBuf),
    Dir,
}

impl Entry {
    fn file_identity(&self) -> Option<(u64, [u8; 32])> {
        match self {
            Self::File(bytes) => Some((bytes.len() as u64, Sha256::digest(bytes).into())),
            Self::FileRef { len, digest, .. } => Some((*len, *digest)),
            _ => None,
        }
    }

    pub(crate) fn bytes(&self) -> Result<std::borrow::Cow<'_, [u8]>> {
        match self {
            Self::File(bytes) => Ok(std::borrow::Cow::Borrowed(bytes)),
            Self::FileRef { source, len, digest } => {
                let bytes = read_regular(source)?;
                require(bytes.len() as u64 == *len && <[u8; 32]>::from(Sha256::digest(&bytes)) == *digest,
                    "captured file changed")?;
                Ok(std::borrow::Cow::Owned(bytes))
            }
            _ => Err("captured entry is not a file".into()),
        }
    }

}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File(a), Self::File(b)) => a == b,
            (Self::File(_) | Self::FileRef { .. }, Self::File(_) | Self::FileRef { .. }) =>
                self.file_identity() == other.file_identity(),
            (Self::Link(a), Self::Link(b)) => a == b,
            (Self::Dir, Self::Dir) => true,
            _ => false,
        }
    }
}
impl Eq for Entry {}

#[derive(Clone, Debug)]
pub struct Store {
    pub path: PathBuf,
    pub nodes: BTreeMap<String, Authored>,
    pub children: BTreeMap<String, Vec<String>>,
    pub roots: Vec<String>,
    pub inventory: BTreeMap<PathBuf, Entry>,
    // Some means content is deliberately absent from Authored; this set records
    // actual nonblank bodies. Rendering/replay must explicitly materialize first.
    pub(crate) compact_bodies: Option<BTreeSet<String>>,
}

fn real_dir(path: &Path) -> Result<()> {
    require(
        fs::symlink_metadata(path)?.is_dir(),
        &format!("not a real directory: {}", path.display()),
    )
}

fn entries(path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(path)? {
        paths.try_reserve(1)?;
        paths.push(entry?.path());
    }
    paths.sort();
    Ok(paths)
}

// Legacy empty homes tracked this marker. It is structural, never a node.
fn node_entries(path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = entries(path)?;
    paths.retain(|p| p.file_name().is_none_or(|n| n != ".keep"));
    let marker = path.join(".keep");
    if fs::symlink_metadata(&marker).is_ok() {
        source_error::context(
            (|| {
                require(
                    read_regular(&marker)?.is_empty(),
                    "nonempty node directory marker",
                )
            })(),
            Path::new("nodes/.keep"),
            None,
        )?;
    }
    Ok(paths)
}

fn read_regular(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    require(
        meta.is_file() && meta.len() <= LIMIT as u64,
        "unsafe or oversized authored file",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        require(meta.nlink() == 1, "hard-linked authored file")?;
    }
    let bytes = fs::read(path)?;
    require(bytes.len() <= LIMIT, "oversized authored file")?;
    Ok(bytes)
}

fn name(path: &Path) -> Result<String> {
    Ok(path
        .file_name()
        .ok_or("missing filename")?
        .to_str()
        .ok_or("non-UTF-8 path")?
        .to_owned())
}

fn safe_link(path: &Path, expected: &Path, destination: &Path) -> Result<()> {
    require(
        fs::symlink_metadata(path)?.file_type().is_symlink(),
        "child/root entry must be a symlink",
    )?;
    require(
        fs::read_link(path)? == expected,
        "unsafe child/root link target",
    )?;
    require(
        fs::canonicalize(path)? == destination,
        "link does not name its canonical node",
    )
}

#[cfg(test)]
thread_local! {
    static INVENTORY_SPAWN_FAILURE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static INVENTORY_IO_FAILURE: std::cell::Cell<Option<std::io::ErrorKind>> = const { std::cell::Cell::new(None) };
}

fn inventory(path: &Path, root: &Path, result: &mut BTreeMap<PathBuf, Entry>) -> Result<()> {
    let paths = entries(path)?;
    if path == root.join("nodes") && paths.len() >= 64 {
        // Only disjoint node subtrees run concurrently; complete physical
        // sweeps and both real byte inventories still bracket this work.
        let (startup, batches) = thread::scope(|scope| {
            let mut workers = Vec::new();
            let mut startup = Ok(());
            for chunk in paths.chunks(paths.len().div_ceil(4)) {
                #[cfg(test)]
                if INVENTORY_SPAWN_FAILURE.with(|failure| failure.get() == Some(workers.len())) {
                    startup = Err(std::io::Error::other("injected inventory spawn failure"));
                    break;
                }
                #[cfg(test)]
                let io_failure = INVENTORY_IO_FAILURE.with(|failure| failure.get());
                match thread::Builder::new().spawn_scoped(scope, move || {
                    let mut batch = BTreeMap::new();
                    let mut read = || -> Result<()> {
                        #[cfg(test)]
                        if let Some(kind) = io_failure {
                            return Err(std::io::Error::from(kind).into());
                        }
                        inventory_entries(chunk, root, &mut batch, false)
                    };
                    read()
                        .map(|()| batch)
                        // Box<dyn Error> is not Send. Keep IO identity intact;
                        // fixed native guard errors retain their exact strings.
                        .map_err(source_error::WorkerError::capture)
                }) {
                    Ok(worker) => workers.push(worker),
                    Err(error) => {
                        startup = Err(error);
                        break;
                    }
                }
            }
            let batches = workers
                .into_iter()
                .map(|worker| worker.join())
                .collect::<Vec<_>>();
            (startup, batches)
        });
        // Drain every worker before returning any error or accepting bytes.
        startup?;
        for batch in batches {
            let batch = batch
                .map_err(|_| "inventory worker panicked")?
                .map_err(source_error::WorkerError::restore)?;
            result.extend(batch);
        }
        Ok(())
    } else {
        inventory_entries(&paths, root, result, false)
    }
}

fn inventory_entries(
    paths: &[PathBuf],
    root: &Path,
    result: &mut BTreeMap<PathBuf, Entry>,
    compact: bool,
) -> Result<()> {
    for p in paths {
        let rel = p.strip_prefix(root)?.to_path_buf();
        source_error::context(
            (|| -> Result<()> {
                if rel == Path::new(".git") {
                    return Ok(());
                }
                if rel == Path::new("nodes/.keep") {
                    let bytes = read_regular(p)?;
                    require(bytes.is_empty(), "nonempty node directory marker")?;
                    result.insert(rel.clone(), Entry::File(bytes));
                    return Ok(());
                }
                // SQLite is derived, can exceed the authored-file limit, and changes
                // during refresh. Never feed cache bytes back into authored validation.
                if [
                    ".sifttext/index.sqlite",
                    ".sifttext/index.sqlite-journal",
                    ".sifttext/index.sqlite-wal",
                    ".sifttext/index.sqlite-shm",
                    ".sifttext/refresh.lock",
                ]
                .iter()
                .any(|cache| rel == Path::new(cache))
                {
                    let meta = fs::symlink_metadata(&p)?;
                    require(meta.is_file(), "unsafe derived cache entry")?;
                    return Ok(());
                }
                if token_metadata::derived(&rel) && token_metadata::safe_derived(&p)? {
                    return Ok(());
                }
                if outlines::derived(&rel) {
                    outlines::safe_derived(&p)?;
                    return Ok(());
                }
                let parts: Vec<_> = rel.components().collect();
                if parts.len() == 3
                    && parts[0].as_os_str() == "nodes"
                    && uuid(parts[1].as_os_str().to_str().unwrap_or_default())
                    && parts[2].as_os_str() == "artifacts"
                {
                    // Node-owned local work is not knowledge. Check only the container:
                    // never traverse, hash, copy or serialize files/projects/link targets.
                    real_dir(&p)?;
                    return Ok(());
                }
                if rel.parent() == Some(Path::new("nodes")) {
                    real_dir(&p)?;
                    require(
                        uuid(&name(&p)?),
                        "node directory is not a canonical full UUID",
                    )?;
                }
                let authored_file = parts.len() == 3
                    && parts[0].as_os_str() == "nodes"
                    && ["node.md", "children.order"]
                        .iter()
                        .any(|n| parts[2].as_os_str() == *n);
                let meta = fs::symlink_metadata(&p)?;
                let file = || -> Result<Entry> {
                    let bytes = read_regular(p)?;
                    Ok(if compact {
                        Entry::FileRef {
                            source: p.clone(), len: bytes.len() as u64,
                            digest: Sha256::digest(&bytes).into(),
                        }
                    } else { Entry::File(bytes) })
                };
                let e = if authored_file {
                    file()?
                } else if meta.file_type().is_symlink() {
                    // Only relationship directories may contain links, already checked individually.
                    let parts: Vec<_> = rel.components().collect();
                    require(
                        (parts.len() == 2 && parts[0].as_os_str() == "trees")
                            || (parts.len() == 4
                                && parts[0].as_os_str() == "nodes"
                                && parts[2].as_os_str() == "children"),
                        "unsafe unrelated symlink",
                    )?;
                    Entry::Link(fs::read_link(&p)?)
                } else if meta.is_dir() {
                    if compact {
                        inventory_entries(&entries(p)?, root, result, true)?;
                    } else {
                        inventory(&p, root, result)?;
                    }
                    Entry::Dir
                } else {
                    file()?
                };
                result.insert(rel.clone(), e);
                Ok(())
            })(),
            &rel,
            None,
        )?;
    }
    Ok(())
}

impl Store {
    pub fn read(path: &Path) -> Result<Self> {
        Self::read_view(path, None)
    }

    /// Compose complete immutable base nodes with whole-node sparse overrides.
    /// A missing overlay node is inherited, never deleted or hydrated on disk.
    pub fn read_view(path: &Path, base: Option<&Store>) -> Result<Self> {
        Ok(Self::capture_view(path, base)?.0)
    }

    // Keep ephemeral metadata with its capture, preserving Store's public shape.
    pub(crate) fn capture_view(
        path: &Path,
        base: Option<&Store>,
    ) -> Result<(Self, coherent_capture::Metadata)> {
        real_dir(path)?;
        let path = fs::canonicalize(path)?;
        let residues = coherent_capture::residues(&path)?;
        Self::capture_classified(&path, base, &residues)
    }

    pub(crate) fn capture_classified(
        path: &Path,
        base: Option<&Store>,
        residues: &coherent_capture::Residues,
    ) -> Result<(Self, coherent_capture::Metadata)> {
        Self::capture_classified_mode(path, base, residues, false)
    }

    pub(crate) fn capture_classified_mode(
        path: &Path,
        base: Option<&Store>,
        residues: &coherent_capture::Residues,
        compact: bool,
    ) -> Result<(Self, coherent_capture::Metadata)> {
        real_dir(path)?;
        let path = fs::canonicalize(path)?;
        let (files, dirs) = Self::inventory_classified_mode(
            &path,
            base.map(|b| (b.path.as_path(), &b.inventory)),
            residues,
            compact,
        )?;
        let captured = coherent_capture::from_inventory_mode(path, files, dirs, base.is_some(), compact)?;
        residues.unchanged()?;
        Ok(captured)
    }

    // Shared by initial parsing and freshness-only recapture. Sparse inheritance
    // selects whole nodes from inventoried directories, including their assets.
    pub(crate) fn inventory_classified_mode(
        path: &Path,
        base: Option<(&Path, &BTreeMap<PathBuf, Entry>)>,
        residues: &coherent_capture::Residues,
        compact: bool,
    ) -> Result<(BTreeMap<PathBuf, Entry>, BTreeMap<String, PathBuf>)> {
        real_dir(path)?;
        real_dir(&path.join("nodes"))?;
        real_dir(&path.join("trees"))?;
        residues.unchanged()?;
        #[cfg(test)]
        coherent_capture::count(|c| c.inventories += 1);
        let mut files = BTreeMap::new();
        if compact {
            inventory_entries(&entries(path)?, path, &mut files, true)?;
        } else {
            inventory(path, path, &mut files)?;
        }
        files.retain(|p, _| {
            let mut parts = p.components();
            !(parts.next().is_some_and(|p| p.as_os_str() == "nodes")
                && parts
                    .next()
                    .and_then(|p| p.as_os_str().to_str())
                    .is_some_and(|id| residues.contains(id)))
        });
        residues.unchanged()?;
        #[cfg(test)]
        coherent_capture::after_inventory();
        let mut dirs = BTreeMap::new();
        if let Some((base_path, base_files)) = base {
            for (rel, entry) in base_files {
                if rel.parent() == Some(Path::new("nodes")) && rel != Path::new("nodes/.keep") {
                    require(matches!(entry, Entry::Dir), "not a real node directory")?;
                    dirs.insert(name(rel)?, base_path.join(rel));
                }
            }
        }
        for (rel, entry) in &files {
            if rel.parent() == Some(Path::new("nodes")) && rel != Path::new("nodes/.keep") {
                require(matches!(entry, Entry::Dir), "not a real node directory")?;
                dirs.insert(name(rel)?, path.join(rel));
            }
        }
        if let Some((base_path, base_files)) = base {
            for (rel, entry) in base_files {
                let parts: Vec<_> = rel.components().collect();
                if parts.len() >= 2 && parts[0].as_os_str() == "nodes" {
                    let id = parts[1].as_os_str().to_str().ok_or("non-UTF-8 node")?;
                    if dirs.get(id).is_some_and(|dir| dir.starts_with(base_path)) {
                        files.insert(rel.clone(), entry.clone());
                    }
                }
            }
        }
        residues.unchanged()?;
        Ok((files, dirs))
    }

    pub fn resolve(&self, prefix: &str) -> Result<String> {
        require(!prefix.is_empty(), "empty UUID prefix")?;
        let matches: Vec<_> = self
            .nodes
            .keys()
            .filter(|id| id.starts_with(prefix))
            .cloned()
            .collect();
        require(matches.len() == 1, "UUID prefix absent or ambiguous")?;
        Ok(matches[0].clone())
    }

    pub fn view(&self) -> serde_json::Value {
        assert!(self.compact_bodies.is_none(), "rendering requires a full Store capture");
        let node_path = |id: &str| self.path.join(format!("nodes/{id}/node.md"));
        let nodes: Vec<_> = self.nodes.values().map(|n| {
            let navigation: Vec<_> = n.see_also.iter().filter_map(|r| self.nodes.get(&r.target).map(|t|
                serde_json::json!({"target": r.target, "name": t.name, "micro_summary": t.micro_summary, "reason": r.reason, "path": node_path(&t.id)}))).collect();
            serde_json::json!({"id": n.id, "name": n.name, "micro_summary": n.micro_summary,
                "content": n.content, "children": self.children[&n.id], "see_also": n.see_also, "navigation": navigation, "path": node_path(&n.id)})
        }).collect();
        serde_json::json!({"roots": self.roots, "nodes": nodes})
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WireNode {
    id: usize,
    payload: usize,
    children: Vec<usize>,
    refs: Vec<(usize, usize)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Graph {
    assets: usize,
    roots: Vec<usize>,
    nodes: Vec<WireNode>,
}

// Shape.valid proves erasure of payload/assets and reference reason zero/nonzero
// normalization preserve Spec.valid. Every authored reference has a positive reason.
fn encode_shape(s: &Store) -> Graph {
    let symbols: HashMap<_, _> = s.nodes.keys().enumerate()
        .map(|(i, id)| (id.as_str(), i + 1)).collect();
    let id = |s: &str| symbols.get(s).copied().unwrap_or(0);
    Graph {
        assets: 0,
        roots: s.roots.iter().map(|s| id(s)).collect(),
        nodes: s.nodes.values().map(|n| WireNode {
            id: id(&n.id), payload: 0,
            children: s.children[&n.id].iter().map(|s| id(s)).collect(),
            refs: n.see_also.iter().map(|r| (id(&r.target), 1)).collect(),
        }).collect(),
    }
}

// Hash the exact normalized shape sent to the same compiled full validator.
fn graph_validation_key(graph: &Graph, policy: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"sifttext-valid-graph-v1\0");
    hash.update(policy.as_bytes());
    let mut word = |value: usize| hash.update((value as u64).to_le_bytes());
    word(0); // erased assets
    word(graph.roots.len());
    for &root in &graph.roots {
        word(root);
    }
    word(graph.nodes.len());
    for node in &graph.nodes {
        word(node.id);
        word(0); // erased payload
        word(node.children.len());
        for &child in &node.children {
            word(child);
        }
        word(node.refs.len());
        for &(target, reason) in &node.refs {
            word(target);
            word(reason);
        }
    }
    format!("{:x}", hash.finalize())
}

// Optional, reconstructible local performance cache, not publication authority.
// Git common metadata shares successful checks across accepted/draft worktrees.
fn graph_cache_dir(store: &Path) -> Result<PathBuf> {
    let root = fs::canonicalize(store)?;
    let (parent, cache_name) = match fs::symlink_metadata(root.join(".git")) {
        Ok(meta) => {
            require(meta.is_dir() || meta.is_file(), "unsafe Git metadata entry")?;
            let common = PathBuf::from(local::reader_git(
                &root,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            )?);
            require(fs::canonicalize(&common)? == common, "unsafe Git metadata path")?;
            real_dir(&common)?;
            (common, "sifttext-valid-graphs")
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // No cache inside authored standalone stores: checking must not mutate
            // their captured source. Git-backed drafts share immutable-input results.
            return Err("graph cache requires Git metadata".into());
        }
        Err(error) => return Err(error.into()),
    };
    let directory = parent.join(cache_name);
    // Only create these two known levels; never follow an existing symlink.
    for path in [&parent, &directory] {
        match fs::create_dir(path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error.into()),
        }
        real_dir(path)?;
    }
    Ok(directory)
}

fn graph_cache_marker(key: &str) -> String {
    format!("sifttext-valid-graph-v1 {key}\n")
}

fn graph_cache_hit(directory: &Path, key: &str) -> Result<bool> {
    real_dir(directory)?;
    let path = directory.join(key);
    let meta = fs::symlink_metadata(&path)?;
    let expected = graph_cache_marker(key);
    require(meta.is_file() && meta.len() == expected.len() as u64, "unsafe graph cache marker")?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        require(meta.nlink() == 1, "hard-linked graph cache marker")?;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    require(file.metadata()?.is_file(), "unsafe graph cache marker")?;
    let mut bytes = Vec::new();
    file.take(expected.len() as u64 + 1).read_to_end(&mut bytes)?;
    Ok(bytes == expected.as_bytes())
}

fn graph_cache_record(directory: &Path, key: &str) -> Result<()> {
    real_dir(directory)?;
    let mut marker = tempfile::NamedTempFile::new_in(directory)?;
    marker.write_all(graph_cache_marker(key).as_bytes())?;
    // Atomic no-clobber: concurrent success for this key is harmless. Never
    // overwrite a malformed/symlink entry; it simply remains a cache miss.
    marker.persist_noclobber(directory.join(key))?;
    // ponytail: one tiny marker per successful structural graph, no GC yet.
    Ok(())
}

fn graph_word_count(g: &Graph) -> Result<usize> {
    let mut count = 3usize
        .checked_add(g.roots.len())
        .ok_or("graph size overflow")?;
    for n in &g.nodes {
        count = count
            .checked_add(4)
            .and_then(|c| c.checked_add(n.children.len()))
            .and_then(|c| n.refs.len().checked_mul(2).and_then(|r| c.checked_add(r)))
            .ok_or("graph size overflow")?;
    }
    Ok(count)
}

fn policy_budget(nodes: usize, items: usize) -> Result<Duration> {
    // The frozen evaluator repeatedly scans nodes for hierarchy/reference checks.
    // Keep the small-request deadline; allow bounded quadratic work, not an
    // untrusted wire-supplied timeout. Saturation is a time cap, not acceptance.
    let micros = nodes
        .checked_mul(items)
        .and_then(|work| work.checked_mul(30))
        .ok_or("policy deadline size overflow")?;
    if nodes <= 256 {
        return Ok(PROCESS_DEADLINE);
    }
    let budget = PROCESS_DEADLINE
        .checked_add(Duration::from_micros(u64::try_from(micros)?))
        .ok_or("policy deadline duration overflow")?;
    Ok(budget.min(MAX_POLICY_DEADLINE))
}

fn policy_deadline(graph: &Graph) -> Result<Duration> {
    let mut items = graph
        .nodes
        .len()
        .checked_add(graph.roots.len())
        .ok_or("policy deadline size overflow")?;
    for node in &graph.nodes {
        items = items
            .checked_add(node.children.len())
            .and_then(|n| n.checked_add(node.refs.len()))
            .ok_or("policy deadline size overflow")?;
    }
    policy_budget(graph.nodes.len(), items)
}

fn graph_words(g: &Graph) -> Result<Vec<usize>> {
    let mut v = reserved(graph_word_count(g)?)?;
    v.extend([g.assets, g.roots.len()]);
    v.extend(&g.roots);
    v.push(g.nodes.len());
    for n in &g.nodes {
        v.extend([n.id, n.payload, n.children.len()]);
        v.extend(&n.children);
        v.push(n.refs.len());
        for (t, r) in &n.refs {
            v.extend([t, r]);
        }
    }
    Ok(v)
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn candidate_files(project: &Path) -> Result<BTreeSet<String>> {
    fn walk(project: &Path, dir: &Path, files: &mut BTreeSet<String>) -> Result<()> {
        for path in entries(dir)? {
            let meta = fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                walk(project, &path, files)?;
            } else {
                require(meta.is_file(), "unsafe candidate source entry")?;
                files.insert(
                    path.strip_prefix(project)?
                        .to_str()
                        .ok_or("non-UTF-8 candidate path")?
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
    let mut files: BTreeSet<String> = ["Cargo.toml", "Cargo.lock", "README.md", ".gitignore"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    if project.join("rust-toolchain.toml").exists() {
        files.insert("rust-toolchain.toml".to_owned());
    }
    for dir in ["src", "laws", "scripts", "tests", "fixtures", "docs"] {
        let path = project.join(dir);
        if path.exists() {
            real_dir(&path)?;
            walk(project, &path, &mut files)?;
        }
    }
    Ok(files)
}

/// Fixed prefix of the native build-stamp guard. Its message names the changed
/// path, so refusal classification matches this prefix instead of a full sentence.
pub const CANDIDATE_CHANGED_PREFIX: &str = "checked candidate changed: ";

pub struct Policy {
    executable: PathBuf,
    project: PathBuf,
}
impl Policy {
    pub fn checked(project: &Path) -> Result<Self> {
        let policy = Self {
            executable: project.join("build/graph-policy"),
            project: project.to_path_buf(),
        };
        policy.verify_stamp()?;
        Ok(policy)
    }

    fn verify_stamp(&self) -> Result<()> {
        let project = &self.project;
        for manifest in ["build/candidate.sha256", "build/graph-policy.sha256", "build/graph-transition.sha256"] {
            let contents = fs::read_to_string(project.join(manifest)).map_err(|error| {
                // Keep the OS error kind so an absent stamp is not reported as unknown.
                std::io::Error::new(
                    error.kind(),
                    format!("cannot read build stamp {manifest}; run sh scripts/build.sh"),
                )
            })?;
            require(!contents.trim().is_empty(), "empty build stamp")?;
            if manifest.ends_with("sha256") && manifest != "build/candidate.sha256" {
                let expected_path = manifest.trim_end_matches(".sha256");
                require(
                    contents.lines().count() == 1
                        && contents
                            .lines()
                            .next()
                            .is_some_and(|l| l.ends_with(&format!("  {expected_path}"))),
                    "incorrect executable stamp",
                )?;
            }
            let mut stamped = BTreeSet::new();
            for line in contents.lines() {
                let (expected, file) = line.split_once("  ").ok_or("malformed build stamp")?;
                require(
                    !Path::new(file).is_absolute() && !file.split('/').any(|c| c == ".."),
                    "unsafe build stamp path",
                )?;
                require(
                    stamped.insert(file.to_owned()),
                    "duplicate build stamp entry",
                )?;
                let digest = (|| -> std::io::Result<String> {
                    let mut input = fs::File::open(project.join(file))?;
                    let mut hash = Sha256::new();
                    let mut buffer = [0u8; 32 * 1024];
                    loop {
                        let count = input.read(&mut buffer)?;
                        if count == 0 { break; }
                        hash.update(&buffer[..count]);
                    }
                    Ok(format!("{:x}", hash.finalize()))
                })().map_err(|error| {
                    // Keep the OS error kind so refusal classification stays truthful.
                    std::io::Error::new(
                        error.kind(),
                        format!("cannot read stamped path {file}; run sh scripts/build.sh"),
                    )
                })?;
                if digest != expected {
                    return Err(format!(
                        "{CANDIDATE_CHANGED_PREFIX}{file}; rebuild and recheck with sh scripts/build.sh"
                    ).into());
                }
            }
            if manifest == "build/candidate.sha256" {
                require(
                    stamped == candidate_files(project)?,
                    "candidate file set changed; rebuild and recheck",
                )?;
            }
        }
        Ok(())
    }

    /// Same-invocation validator identity, including the native graph executable
    /// stamp and canonical policy home, not only a replaceable source manifest.
    /// This only pins identity; native callers still verify the checked stamps.
    pub(crate) fn validation_identity(&self) -> Result<String> {
        let mut bytes = Vec::new();
        for value in [
            fs::canonicalize(&self.project)?
                .as_os_str()
                .as_encoded_bytes()
                .to_vec(),
            crate::read_regular(&self.project.join("build/candidate.sha256"))?,
            crate::read_regular(&self.project.join("build/graph-policy.sha256"))?,
        ] {
            bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
            bytes.extend(value);
        }
        Ok(sha256(&bytes))
    }

    fn call(&self, graph: &Graph) -> Result<String> {
        self.verify_stamp()?;
        let count = graph_word_count(graph)?
            .checked_add(4)
            .ok_or("graph size overflow")?;
        let mut words = reserved(count)?;
        words.extend([1, 0, 0, 0]);
        words.extend(graph_words(graph)?);
        let wire = wire_text(&words)?;
        invoke_bounded_with_deadline(
            &self.executable,
            wire.as_bytes(),
            "STG1 REJECT\n".len(),
            policy_deadline(graph)?,
        )
    }

    pub(crate) fn graph_valid(&self, s: &Store) -> Result<bool> {
        self.graph_valid_input(&encode_shape(s), &s.path)
    }

    pub(crate) fn graph_valid_input(&self, graph: &Graph, cache_root: &Path) -> Result<bool> {
        let started = Instant::now();
        // A marker never bypasses the checked source/executable stamp guard.
        self.verify_stamp()?;
        // Graph truth depends on this predicate and proved normalization, not
        // unrelated documentation, SQL rendering, or the runtime's install path.
        let predicate = sha256(&[
            fs::read(self.project.join("build/graph-policy.sha256"))?,
            fs::read(self.project.join("laws/successor/validation/Shape.bend"))?,
        ].concat());
        let key = graph_validation_key(graph, &predicate);
        let directory = graph_cache_dir(cache_root).ok();
        let hit = directory.as_ref().is_some_and(|directory| {
            graph_cache_hit(directory, &key).unwrap_or(false)
        });
        let result = if hit {
            Ok(true)
        } else {
            self.call(graph)
                .map_err(|error| {
                    if error.to_string() == "policy process output exceeds limit" {
                        "malformed checked graph verdict".into()
                    } else {
                        error
                    }
                })
                .and_then(|verdict| match verdict.as_str() {
                    "STG1 VALID\n" => Ok(true),
                    "STG1 REJECT\n" => Ok(false),
                    _ => Err("malformed checked graph verdict".into()),
                })
        };
        let recorded = !hit && matches!(result, Ok(true)) && directory.as_ref().is_some_and(|directory| {
            graph_cache_record(directory, &key).is_ok()
        });
        if std::env::var_os("SIFTTEXT_RETRIEVAL_TRACE").is_some() {
            eprintln!(
                "graph-work cache_hit={hit} law_run={} valid={} cache_write={recorded} elapsed_ms={}",
                !hit,
                match &result { Ok(true) => "true", Ok(false) => "false", Err(_) => "error" },
                started.elapsed().as_millis(),
            );
        }
        result
    }

    pub fn validate(&self, s: &Store) -> Result<()> {
        require(
            self.graph_valid(s)?,
            "checked graph rejected or malformed verdict",
        )
    }
}

fn wire_text(words: &[usize]) -> Result<String> {
    let bytes = words
        .iter()
        .try_fold(0usize, |sum, word| {
            sum.checked_add(word.checked_ilog10().unwrap_or(0) as usize + 2)
                .ok_or("graph size overflow")
        })?
        .saturating_sub(1);
    require(
        bytes <= BACKEND_INPUT_BYTES,
        "wire exceeds frozen backend input limit",
    )?;
    let mut wire = String::new();
    wire.try_reserve_exact(bytes)?;
    for (i, word) in words.iter().enumerate() {
        if i != 0 {
            wire.push(' ');
        }
        use std::fmt::Write;
        write!(wire, "{word}")?;
    }
    Ok(wire)
}

fn process_output(mut pipe: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let end = limit
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("output size overflow"))?;
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    while bytes.len() < end {
        let remaining = (end - bytes.len()).min(chunk.len());
        let n = match pipe.read(&mut chunk[..remaining]) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if n == 0 {
            return Ok(bytes);
        }
        bytes.try_reserve(n).map_err(std::io::Error::other)?;
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "policy process output exceeds limit",
            ));
        }
    }
    unreachable!()
}

#[cfg(test)]
fn invoke_bounded(executable: &Path, input: &[u8], stdout_bytes: usize) -> Result<String> {
    invoke_bounded_with_deadline(executable, input, stdout_bytes, PROCESS_DEADLINE)
}

fn invoke_bounded_with_deadline(
    executable: &Path,
    input: &[u8],
    stdout_bytes: usize,
    budget: Duration,
) -> Result<String> {
    require(
        input.len() <= BACKEND_INPUT_BYTES,
        "wire exceeds frozen backend input limit",
    )?;
    stdout_bytes.checked_add(1).ok_or("output size overflow")?;
    require(
        !budget.is_zero() && budget <= MAX_POLICY_DEADLINE,
        "policy deadline exceeds resource budget",
    )?;
    let operation = scratch::Directory::new()?;
    let mut file = tempfile::NamedTempFile::new_in(operation.path())?;
    file.write_all(input)?;
    file.flush()?;
    let mut child = Command::new(executable)
        .env("SIFTTEXT_WIRE", file.path())
        .args(["--threads", "1", "--gpu", "off"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let stderr = child.stderr.take().ok_or("missing stderr")?;
    let (tx, rx) = mpsc::channel();
    for (tag, pipe, limit) in [
        (0, Box::new(stdout) as Box<dyn Read + Send>, stdout_bytes),
        (1, Box::new(stderr) as Box<dyn Read + Send>, 0),
    ] {
        let tx = tx.clone();
        if let Err(error) = thread::Builder::new().spawn(move || {
            let _ = tx.send((tag, process_output(pipe, limit)));
        }) {
            let _ = child.kill();
            child.wait()?;
            return Err(error.into());
        }
    }
    drop(tx);
    let result = (|| -> Result<String> {
        let deadline = Instant::now() + budget;
        let mut status = None;
        let mut output = [None, None];
        loop {
            if status.is_none() {
                status = child.try_wait()?;
            }
            while let Ok((tag, bytes)) = rx.try_recv() {
                output[tag] = Some(match bytes {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        // Closing rejected stdout can cause SIGPIPE; keep its initiating refusal.
                        if tag == 0 && error.kind() == std::io::ErrorKind::InvalidData {
                            return Err("policy process output exceeds limit".into());
                        }
                        thread::sleep(Duration::from_millis(1));
                        if let Some(status) = child.try_wait()? {
                            require(status.success(), "policy process exited nonzero")?;
                        }
                        if error.kind() == std::io::ErrorKind::InvalidData {
                            return Err(if tag == 1 {
                                "unexpected policy stderr"
                            } else {
                                "policy process output exceeds limit"
                            }
                            .into());
                        }
                        return Err(error.into());
                    }
                });
            }
            if let Some(status) = status
                && output[0].is_some()
                && output[1].is_some()
            {
                require(status.success(), "policy process exited nonzero")?;
                return Ok(std::str::from_utf8(&output[0].take().unwrap())?.to_owned());
            }
            if Instant::now() >= deadline {
                return Err("policy process timed out".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        child.wait()?;
    }
    result
}
