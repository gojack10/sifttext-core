//! Disposable outline publication, serialized by the index's SQLite writer transaction.
use crate::{
    Result, Store, outline_artifacts,
    outline_rows::{Coverage, Forest, Node},
    require, sha256,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub version: String,
    pub files: BTreeMap<String, String>,
}
struct File {
    relative: String,
    hash: String,
    staged: Option<(tempfile::TempPath, (u64, u64, i64, i64))>,
    parent_identity: (u64, u64),
    target_identity: Option<(u64, u64, i64, i64)>,
}
impl File {
    fn checked_bytes(&self, root: &Path) -> Result<Vec<u8>> {
        let target = root.join(&self.relative);
        let (path, expected) = match &self.staged {
            Some((path, identity)) => (path.as_ref(), Some(*identity)),
            None => (target.as_path(), self.target_identity),
        };
        require(
            file_identity(path)? == expected,
            "changed artifact identity",
        )?;
        let bytes = fs::read(path)?;
        require(sha256(&bytes) == self.hash, "changed artifact bytes")?;
        require(
            file_identity(path)? == expected,
            "changed artifact identity",
        )?;
        Ok(bytes)
    }
}
pub(crate) struct Publication {
    stage: Option<PathBuf>,
    files: Vec<File>,
    pub manifest: Manifest,
}
fn directory(path: &Path) -> Result<(u64, u64)> {
    crate::real_dir(path)?;
    let m = fs::symlink_metadata(path)?;
    Ok((m.dev(), m.ino()))
}
fn mkdir(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    directory(path)?;
    Ok(())
}
fn file_identity(path: &Path) -> Result<Option<(u64, u64, i64, i64)>> {
    match fs::symlink_metadata(path) {
        Ok(m) => {
            require(m.is_file() && m.nlink() == 1, "unsafe generated file")?;
            Ok(Some((m.dev(), m.ino(), m.ctime(), m.ctime_nsec())))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
/// Only known generated paths are excluded from authored inventory. Never follow a link.
pub(crate) fn derived(relative: &Path) -> bool {
    let p: Vec<_> = relative.components().collect();
    (p.len() >= 2
        && p[0].as_os_str() == ".sifttext"
        && ["outline-stage", "outlines"]
            .iter()
            .any(|s| p[1].as_os_str() == *s))
        || (p.len() == 3
            && p[0].as_os_str() == "nodes"
            && [
                "outline.lean",
                "outline.full.lean",
                "outline.lean.meta.json",
                "outline.full.lean.meta.json",
            ]
            .iter()
            .any(|s| p[2].as_os_str() == *s))
}
pub(crate) fn safe_derived(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if m.is_dir() {
        for p in crate::entries(path)? {
            safe_derived(&p)?;
        }
        Ok(())
    } else {
        require(m.is_file() && m.nlink() == 1, "unsafe generated entry")
    }
}
// Version the native processing boundary separately from global authored freshness.
// Pure supplied-version assemblers retain their existing API and exact supplied bytes.
fn processing_identity() -> String {
    sha256(
        format!(
            "native-outlines/2;{};{};{};{}",
            crate::token_metadata::TOKENIZER_VERSION,
            include_str!("outline_rows.rs"),
            include_str!("outline_artifacts.rs"),
            include_str!("lean_render.rs"),
        )
        .as_bytes(),
    )
}
fn prefix(store: &Store, id: &str) -> Result<String> {
    // Never hydrate sparse omissions or follow generated writes into their immutable base.
    Ok(if store.path.join("nodes").join(id).try_exists()? {
        format!("nodes/{id}")
    } else {
        format!(".sifttext/outlines/{id}")
    })
}
// Decode only the generated scalar/tabular subset of the pinned dialect. This
// is a verifier, not a second encoder; canonical bytes are checked by the existing
// renderer, and every decoded field is compared with the current authored graph.
fn scalar(text: &str, cell: bool) -> Result<Value> {
    if text.starts_with('"') {
        require(text.ends_with('"') && text.len() >= 2, "invalid LEAN quote")?;
        let mut out = String::new();
        let mut chars = text[1..text.len() - 1].chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' => out.push(match chars.next().ok_or("unterminated LEAN escape")? {
                    'n' => '\n',
                    '\\' => '\\',
                    '"' if !cell => '"',
                    _ => return Err("invalid LEAN escape".into()),
                }),
                '"' if cell && chars.next() == Some('"') => out.push('"'),
                '"' => return Err("invalid LEAN quote".into()),
                _ => out.push(c),
            }
        }
        Ok(json!(out))
    } else {
        Ok(match text {
            "T" => json!(true),
            "F" => json!(false),
            "_" => Value::Null,
            _ => serde_json::from_str::<serde_json::Number>(text)
                .map(Value::Number)
                .unwrap_or_else(|_| json!(text)),
        })
    }
}
fn cells(text: &str) -> Result<Vec<&str>> {
    let mut parts = Vec::new();
    let (mut quoted, mut escaped, mut start) = (false, false, 0);
    for (i, c) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && quoted {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == '\t' && !quoted {
            parts.push(&text[start..i]);
            start = i + 1;
        }
    }
    require(!quoted && !escaped, "unterminated LEAN cell")?;
    parts.push(&text[start..]);
    Ok(parts)
}
fn decode(text: &str) -> Result<Value> {
    let mut root = serde_json::Map::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if let Some(header) = line.strip_prefix("rows[") {
            let (count, columns) = header.split_once("]:").ok_or("invalid LEAN rows")?;
            let count: usize = count.parse()?;
            let mut columns: Vec<_> = columns.split('\t').collect();
            let optional = columns.last() == Some(&"~");
            if optional {
                columns.pop();
            }
            let mut rows = Vec::new();
            for _ in 0..count {
                let line = lines
                    .next()
                    .ok_or("missing LEAN row")?
                    .strip_prefix("  ")
                    .ok_or("invalid LEAN row")?;
                let parts = cells(line)?;
                require(parts.len() >= columns.len(), "missing LEAN cell")?;
                let mut row = serde_json::Map::new();
                for (k, v) in columns.iter().zip(&parts) {
                    require(
                        row.insert((*k).to_owned(), scalar(v, true)?).is_none(),
                        "duplicate LEAN cell",
                    )?;
                }
                for part in &parts[columns.len()..] {
                    require(optional, "extra LEAN cell")?;
                    let (k, v) = part.split_once(':').ok_or("invalid optional cell")?;
                    require(
                        row.insert(k.to_owned(), scalar(v, true)?).is_none(),
                        "duplicate LEAN cell",
                    )?;
                }
                rows.push(Value::Object(row));
            }
            require(
                root.insert("rows".into(), json!(rows)).is_none(),
                "duplicate LEAN rows",
            )?;
        } else {
            let (key, value) = line.split_once(':').ok_or("invalid LEAN metadata")?;
            require(
                root.insert(key.into(), scalar(value, false)?).is_none(),
                "duplicate LEAN metadata",
            )?;
        }
    }
    // The generated codec boundary has lexicographic root keys and a final LF.
    let mut canonical = Vec::new();
    for (key, value) in &root {
        canonical.push(crate::lean_render::render(&json!({key:value}))?);
    }
    require(
        canonical.join("\n") + "\n" == text,
        "noncanonical generated LEAN",
    )?;
    Ok(Value::Object(root))
}
impl Publication {
    pub(crate) fn outline_text(&self, store: &Store, id: &str, full: bool) -> Result<String> {
        let name = if full {
            "outline.full.lean"
        } else {
            "outline.lean"
        };
        let relative = format!("{}/{name}", prefix(store, id)?);
        let file = self
            .files
            .iter()
            .find(|file| file.relative == relative)
            .ok_or("no generated outline for this node; read its node.md")?;
        Ok(String::from_utf8(file.checked_bytes(&store.path)?)?)
    }

    pub(crate) fn reused(&self) -> bool {
        self.stage.is_none()
    }
    /// Reuse is an authored-row/processing/byte/cost check, not trust in an editable cache.
    /// Global freshness may advance while every per-file identity remains unchanged.
    pub(crate) fn reuse(
        store: &Store,
        nodes: &[Value],
        version: &str,
        manifest: &Manifest,
    ) -> Result<Self> {
        let publication = Self::build(store, nodes, version, false, false)?;
        require(
            publication.manifest.files == manifest.files,
            "changed outline manifest",
        )?;
        Ok(publication)
    }
    /// Explicit full/fault-injection preparation retains real staged candidates.
    pub(crate) fn prepare(store: &Store, nodes: &[Value], version: &str) -> Result<Self> {
        Self::prepare_changed(store, nodes, version, true)
    }
    /// Called only while holding the existing SQLite IMMEDIATE transaction. Unchanged
    /// complete artifacts are verified in place; only changed/missing outputs are staged.
    pub(crate) fn prepare_changed(
        store: &Store,
        nodes: &[Value],
        version: &str,
        force_stage: bool,
    ) -> Result<Self> {
        let stage = store.path.join(".sifttext/outline-stage");
        if stage.try_exists()? {
            safe_derived(&stage)?;
            require(
                fs::symlink_metadata(&stage)?.is_dir(),
                "unsafe stage directory",
            )?;
            fs::remove_dir_all(&stage)?;
        }
        Self::build(store, nodes, version, true, force_stage)
    }
    fn build(
        store: &Store,
        nodes: &[Value],
        version: &str,
        repair: bool,
        force_stage: bool,
    ) -> Result<Self> {
        let mut rows = nodes.to_vec();
        for row in &mut rows {
            row["crystallization"] = row["content"].clone();
        }
        let typed: Vec<Node> = serde_json::from_value(json!(rows))?;
        let forest = Forest::new(&typed)?;
        let processing = processing_identity();
        let full_ids = forest.full_identities(&processing)?;
        let mut costs = BTreeMap::new();
        let mut publication = Self {
            stage: None,
            files: Vec::new(),
            manifest: Manifest {
                version: version.to_owned(),
                files: BTreeMap::new(),
            },
        };
        // Full outputs are consumed immediately, retaining only costs/identities, never
        // a forest-sized set of complete text. Shallow costs depend on these exact full bytes.
        for (coverage, name) in [
            (Coverage::Full, "outline.full.lean"),
            (Coverage::Shallow, "outline.lean"),
        ] {
            for node in &typed {
                let id = node.id.as_str();
                let mut expected =
                    serde_json::to_value(forest.project(id, coverage, &full_ids[id])?)?;
                let mut branches = serde_json::Map::new();
                if coverage == Coverage::Shallow {
                    // The shallow identity includes exactly displayed rows and full-view
                    // costs, not a child's unrelated authored body or global revision.
                    expected.as_object_mut().unwrap().remove("source_version");
                    for row in expected["rows"].as_array_mut().ok_or("missing rows")? {
                        let child = row["id"].as_str().ok_or("missing row id")?.to_owned();
                        let count = costs[&child];
                        row["full_view_tokens"] = json!(count);
                        if child != id {
                            branches.insert(child, json!(count));
                        }
                    }
                    expected["source_version"] =
                        json!(sha256(&serde_json::to_vec(&(&processing, &expected))?));
                }
                let prefix = prefix(store, id)?;
                if repair && prefix.starts_with(".sifttext/") {
                    mkdir(&store.path.join(".sifttext/outlines"))?;
                    mkdir(&store.path.join(&prefix))?;
                }
                let relative = format!("{prefix}/{name}");
                let target = store.path.join(&relative);
                let parent_identity = directory(target.parent().ok_or("missing parent")?)?;
                let target_identity = file_identity(&target)?;
                let old = if target_identity.is_some() {
                    Some(fs::read(&target)?)
                } else {
                    None
                };
                require(
                    file_identity(&target)? == target_identity,
                    "changed artifact during verification",
                )?;
                let verified = old.as_deref().is_some_and(|b| {
                    std::str::from_utf8(b)
                        .ok()
                        .and_then(|t| decode(t).ok())
                        .as_ref()
                        == Some(&expected)
                });
                require(
                    repair || verified,
                    "stale or forged outline records/identity",
                )?;
                let bytes = if verified {
                    old.as_ref().unwrap().clone()
                } else {
                    outline_artifacts::bytes(&expected)?.into_bytes()
                };
                let (hash, tokens) = outline_artifacts::cost(std::str::from_utf8(&bytes)?);
                let mut meta = json!({"hash":hash,"tokens":tokens,"encoding":"o200k_base"});
                if coverage == Coverage::Full {
                    costs.insert(id.to_owned(), tokens);
                } else {
                    meta["full_view_tokens"] = json!(costs[id]);
                    meta["branch_full_tokens"] = json!(branches);
                }
                publication.add(
                    store,
                    relative,
                    &bytes,
                    old.as_deref(),
                    parent_identity,
                    target_identity,
                    repair,
                    force_stage,
                )?;
                let relative = format!("{prefix}/{name}.meta.json");
                let target = store.path.join(&relative);
                let target_identity = file_identity(&target)?;
                let old = if target_identity.is_some() {
                    Some(fs::read(&target)?)
                } else {
                    None
                };
                require(
                    file_identity(&target)? == target_identity,
                    "changed sidecar during verification",
                )?;
                let bytes = (serde_json::to_string(&meta)? + "\n").into_bytes();
                require(
                    repair || old.as_deref() == Some(&bytes),
                    "forged outline costs",
                )?;
                publication.add(
                    store,
                    relative,
                    &bytes,
                    old.as_deref(),
                    parent_identity,
                    target_identity,
                    repair,
                    force_stage,
                )?;
            }
        }
        // Keep every per-file durability barrier, but allow at most four in flight.
        // All workers settle before any staged candidate reaches acceptance/publication.
        if publication.stage.is_some() {
            std::thread::scope(|scope| -> Result<()> {
                let workers: Vec<_> = (0..4)
                    .map(|offset| {
                        let files = &publication.files;
                        scope.spawn(move || -> std::io::Result<()> {
                            for f in files.iter().skip(offset).step_by(4) {
                                if let Some((path, identity)) = &f.staged {
                                    let check = |m: fs::Metadata| -> std::io::Result<()> {
                                        if m.is_file()
                                            && m.nlink() == 1
                                            && (m.dev(), m.ino(), m.ctime(), m.ctime_nsec())
                                                == *identity
                                        {
                                            Ok(())
                                        } else {
                                            Err(std::io::Error::other("changed staged file"))
                                        }
                                    };
                                    check(fs::symlink_metadata(path)?)?;
                                    let file = fs::OpenOptions::new()
                                        .read(true)
                                        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                                        .open(path)?;
                                    check(file.metadata()?)?;
                                    file.sync_all()?;
                                    check(fs::symlink_metadata(path)?)?;
                                }
                            }
                            Ok(())
                        })
                    })
                    .collect();
                // Join even after a refusal: cleanup must not race an outstanding sync.
                let results: Vec<_> = workers.into_iter().map(|w| w.join()).collect();
                for result in results {
                    result.map_err(|_| "outline synchronization worker panicked")??;
                }
                Ok(())
            })?;
        }
        Ok(publication)
    }
    #[allow(clippy::too_many_arguments)] // one file's checked physical and byte identities
    fn add(
        &mut self,
        store: &Store,
        relative: String,
        bytes: &[u8],
        old: Option<&[u8]>,
        parent_identity: (u64, u64),
        target_identity: Option<(u64, u64, i64, i64)>,
        repair: bool,
        force_stage: bool,
    ) -> Result<()> {
        let staged = if repair && (force_stage || old != Some(bytes)) {
            let stage = store.path.join(".sifttext/outline-stage");
            if self.stage.is_none() {
                mkdir(&stage)?;
                self.stage = Some(stage.clone());
            }
            let mut file = tempfile::NamedTempFile::new_in(&stage)?;
            file.write_all(bytes)?;
            // Close every writer immediately; bounded sync workers run before acceptance.
            // Keep path cleanup and atomic persist.
            let file = file.into_temp_path();
            let identity = file_identity(&file)?.ok_or("missing staged file")?;
            Some((file, identity))
        } else {
            None
        };
        let hash = sha256(bytes);
        self.manifest.files.insert(relative.clone(), hash.clone());
        self.files.push(File {
            relative,
            hash,
            staged,
            parent_identity,
            target_identity,
        });
        Ok(())
    }
    pub(crate) fn staged_files(&self) -> usize {
        self.files.iter().filter(|f| f.staged.is_some()).count()
    }
    pub(crate) fn unchanged(&self, root: &Path) -> Result<()> {
        for f in &self.files {
            let target = root.join(&f.relative);
            require(
                directory(target.parent().ok_or("missing parent")?)? == f.parent_identity,
                "replaced output directory",
            )?;
            require(
                file_identity(&target)? == f.target_identity,
                "changed output candidate",
            )?;
            f.checked_bytes(root)?;
        }
        Ok(())
    }
    /// Individual atomic replacements, NOT a cross-file atomic transaction. The
    /// SQLite manifest is committed last; inspection checks all files under its lock.
    pub(crate) fn publish(self, root: &Path) -> Result<()> {
        self.unchanged(root)?;
        for f in self.files {
            let target = root.join(&f.relative);
            require(
                directory(target.parent().ok_or("missing parent")?)? == f.parent_identity,
                "replaced output directory",
            )?;
            require(
                file_identity(&target)? == f.target_identity,
                "changed output candidate",
            )?;
            let bytes = f.checked_bytes(root)?;
            if let Some((staged, _)) = f.staged
                && fs::read(&target).ok().as_deref() != Some(&bytes)
            {
                staged.persist(&target).map_err(|e| e.error)?;
            }
        }
        if let Some(stage) = self.stage {
            fs::remove_dir(&stage)?;
        }
        Ok(())
    }
}
/// Verify the freshly constructed manifest after replacement and before success.
pub(crate) fn verify_manifest(root: &Path, manifest: &Manifest) -> Result<()> {
    for (relative, hash) in &manifest.files {
        let path = root.join(relative);
        directory(path.parent().ok_or("missing parent")?)?;
        file_identity(&path)?.ok_or("missing generated file")?;
        require(
            sha256(&fs::read(path)?) == *hash,
            "changed published artifact",
        )?;
    }
    Ok(())
}

/// Read a complete set through the manifest. Paths are reconstructed from the
/// validated store, not taken on trust from editable cache JSON.
pub(crate) fn inspect_files(
    store: &Store,
    nodes: &[Value],
    manifest: &Manifest,
    include_files: bool,
) -> Result<Vec<Value>> {
    let publication = Publication::reuse(store, nodes, &manifest.version, manifest)?;
    publication.unchanged(&store.path)?;
    let mut out = Vec::new();
    for file in &publication.files {
        let bytes = file.checked_bytes(&store.path)?;
        let text = std::str::from_utf8(&bytes)?;
        if include_files {
            let path = store.path.join(&file.relative);
            out.push(json!({"path":path,"hash":file.hash,"text":text}));
        }
    }
    publication.unchanged(&store.path)?;
    Ok(out)
}

/// Historical, source-verified generated bytes. This is deliberately not
/// deserializable: an editable sidecar/manifest is not a provenance token.
/// Capture it before reverting, or reconstruct and refresh the historical
/// authored source in a separate disposable store. No currentness is asserted
/// for the store in which residue is subsequently recognized.
pub struct ResidueProof {
    owner: String,
    source_version: String,
    files: BTreeMap<String, (String, usize)>,
}
impl ResidueProof {
    /// Uses the ordinary complete-source/index/decoder/cost verification path.
    /// Missing, invalid, stale or forged source views cannot mint a proof.
    pub fn capture(
        policy: &crate::Policy,
        source: &Path,
        base: Option<&Path>,
        owner: &str,
    ) -> Result<Self> {
        require(crate::uuid(owner), "invalid residue owner")?;
        let checked = inspect(policy, source, base)?;
        require(checked.status == "current", "unverified residue source")?;
        let source = fs::canonicalize(source)?;
        let mut files = BTreeMap::new();
        for file in checked.files {
            let path = Path::new(file["path"].as_str().ok_or("missing artifact path")?);
            for prefix in ["nodes", ".sifttext/outlines"] {
                let parent = source.join(prefix).join(owner);
                if path.parent() == Some(parent.as_path()) {
                    let name = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .ok_or("invalid artifact name")?;
                    let text = file["text"].as_str().ok_or("missing artifact bytes")?;
                    require(
                        files
                            .insert(name.to_owned(), (sha256(text.as_bytes()), text.len()))
                            .is_none(),
                        "duplicate residue artifact",
                    )?;
                }
            }
        }
        require(files.len() == 4, "missing owner outline artifacts")?;
        Ok(Self {
            owner: owner.to_owned(),
            source_version: checked.version.ok_or("missing verified source version")?,
            files,
        })
    }
    /// Historical complete-source identity, not the residue store's current version.
    pub fn source_version(&self) -> &str {
        &self.source_version
    }
    /// Verify ONE resource; never exempt its parent directory or other entries.
    /// Callers must enumerate/classify all resources and preserve unknown ones.
    /// `root` must be an absolute canonical store path, and `relative` exactly
    /// nodes/<proof owner>/<canonical outline or sidecar name>.
    pub fn recognize(&self, root: &Path, relative: &Path) -> Result<RecognizedResidue> {
        require(
            root.is_absolute() && fs::canonicalize(root)? == root,
            "noncanonical residue root",
        )?;
        let name = relative
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid residue path")?;
        let (hash, length) = self.files.get(name).ok_or("unknown residue resource")?;
        require(
            relative == Path::new("nodes").join(&self.owner).join(name),
            "mismatched residue owner/path",
        )?;
        let path = root.join(relative);
        let mut directories = Vec::new();
        for dir in [
            root.to_owned(),
            root.join("nodes"),
            root.join("nodes").join(&self.owner),
        ] {
            directories.push((dir.clone(), residue_directory(&dir)?, crate::entries(&dir)?));
        }
        let identity = file_identity(&path)?.ok_or("missing residue resource")?;
        let residue = RecognizedResidue {
            path,
            directories,
            identity,
            hash: hash.clone(),
            length: *length,
        };
        residue.unchanged()?;
        Ok(residue)
    }
}
fn residue_directory(path: &Path) -> Result<(u64, u64, i64, i64)> {
    directory(path)?;
    let m = fs::symlink_metadata(path)?;
    require(m.is_dir(), "unsafe residue directory")?;
    Ok((m.dev(), m.ino(), m.ctime(), m.ctime_nsec()))
}
type ResidueDirectorySnapshot = (PathBuf, (u64, u64, i64, i64), Vec<PathBuf>);

/// A bounded resource receipt. Hold it through the consumer's source capture
/// and call `unchanged` at its final acceptance barrier. Not deletion authority.
pub struct RecognizedResidue {
    path: PathBuf,
    directories: Vec<ResidueDirectorySnapshot>,
    identity: (u64, u64, i64, i64),
    hash: String,
    length: usize,
}
impl RecognizedResidue {
    fn identities(&self) -> Result<()> {
        for (dir, expected, entries) in &self.directories {
            require(
                residue_directory(dir)? == *expected,
                "changed residue directory",
            )?;
            // Directory ctime can stay identical for successive changes on Linux.
            require(crate::entries(dir)? == *entries, "changed residue entries")?;
        }
        require(
            file_identity(&self.path)? == Some(self.identity),
            "changed residue resource",
        )
    }
    /// Independent before/after physical checks plus exact verified byte identity.
    /// Read at most the known generated length + 1; oversized unknown resources
    /// cannot drive an unbounded allocation or be accepted by truncated hashing.
    pub fn unchanged(&self) -> Result<()> {
        self.identities()?;
        let limit = self
            .length
            .checked_add(1)
            .ok_or("residue length overflow")?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(limit)?;
        fs::File::open(&self.path)?
            .take(u64::try_from(limit)?)
            .read_to_end(&mut bytes)?;
        require(
            bytes.len() == self.length && sha256(&bytes) == self.hash,
            "unverified residue bytes",
        )?;
        self.identities()
    }
}

#[cfg(test)]
#[test]
fn residue_entries_reject_changes_even_with_equal_directory_metadata() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("outline.lean");
    fs::write(&path, "checked").unwrap();
    let mut receipt = RecognizedResidue {
        identity: file_identity(&path).unwrap().unwrap(),
        path,
        directories: vec![(
            root.path().to_owned(),
            residue_directory(root.path()).unwrap(),
            crate::entries(root.path()).unwrap(),
        )],
        hash: sha256(b"checked"),
        length: 7,
    };
    receipt.unchanged().unwrap();
    fs::write(root.path().join("unknown"), "protected").unwrap();
    // Simulate a filesystem reporting the same timestamp for both operations.
    receipt.directories[0].1 = residue_directory(root.path()).unwrap();
    assert_eq!(
        receipt.unchanged().unwrap_err().to_string(),
        "changed residue entries"
    );
}

#[derive(Debug, Serialize)]
pub struct Inspection {
    pub status: &'static str,
    pub version: Option<String>,
    pub files: Vec<Value>,
}
impl Inspection {
    pub(crate) fn unavailable(status: &'static str) -> Self {
        Self {
            status,
            version: None,
            files: vec![],
        }
    }
}
/// Does not refresh, repair or silently accept stale files. Includes exact bytes
/// so readers need not perform a racy second raw read after inspection.
pub fn inspect(policy: &crate::Policy, path: &Path, base: Option<&Path>) -> Result<Inspection> {
    crate::index::inspect_outlines(policy, path, base)
}

// Publication alone can borrow its final successful validation. Ordinary reads
// still validate independently; mismatches never fall back to a graph recheck.
pub(crate) fn inspect_validated(
    policy: &crate::Policy,
    path: &Path,
    validated: &crate::candidate_validation::ValidatedCapture,
) -> Result<Inspection> {
    crate::index::inspect_validated_outlines(policy, path, validated, true)
}

/// Publication consumes only status/version. Keep every byte/identity check but
/// do not allocate the complete forest of artifact texts that it would discard.
pub(crate) fn verify_validated(
    policy: &crate::Policy,
    path: &Path,
    validated: &crate::candidate_validation::ValidatedCapture,
) -> Result<Inspection> {
    crate::index::inspect_validated_outlines(policy, path, validated, false)
}
