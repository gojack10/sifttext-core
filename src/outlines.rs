//! Disposable outline publication. Indexed refresh locking/transactions belong to the caller.
mod indexed;
use crate::{Result, outline_artifacts, require, sha256};
pub(crate) use indexed::{indexed_current, indexed_outline, refresh_indexed_from};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

fn directory(path: &Path) -> Result<(u64, u64)> {
    crate::real_dir(path)?;
    let m = fs::symlink_metadata(path)?;
    Ok((m.dev(), m.ino()))
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
            "native-outlines/3;{};{};{};{};{};{};{};{};{}",
            include_str!("outlines.rs"),
            include_str!("outlines/indexed.rs"),
            include_str!("token_metadata.rs"),
            include_str!("token_count.rs"),
            crate::token_metadata::TOKENIZER_VERSION,
            include_str!("outline_rows.rs"),
            include_str!("outline_artifacts.rs"),
            include_str!("lean_render.rs"),
            include_str!("../Cargo.lock"),
        )
        .as_bytes(),
    )
}
// Decode the generated scalar/tabular subset for requested CLI projection.
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
/// Decode canonical CLI LEAN or per-field generated-outline LEAN.
pub fn decode_lean(text: &str) -> Result<Value> {
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
    let document = Value::Object(root);
    // CLI values use the whole-document renderer; generated outlines encode each sorted field
    // independently so metadata may follow the rows block. Accept either exact canonical form.
    let rendered = crate::lean_render::render(&document)? + "\n";
    let outline = outline_artifacts::bytes(&document)?;
    require(
        rendered == text || outline == text,
        "noncanonical generated LEAN",
    )?;
    Ok(document)
}
fn cli_document(text: &str, version: &str) -> Result<Value> {
    let mut document = decode_lean(text)?;
    let fields = document.as_object_mut().ok_or("invalid outline document")?;
    for key in ["source_version", "source_version_scope", "token_encoding"] {
        fields.remove(key);
    }
    fields.insert("version".into(), json!(version));
    for row in document["rows"].as_array_mut().ok_or("missing outline rows")? {
        let fields = row.as_object_mut().ok_or("invalid outline row")?;
        for key in ["descendants", "full_view_tokens"] {
            fields.remove(key);
        }
    }
    Ok(document)
}

/// Historical native generated bytes, not a deserializable provenance token.
/// Capture from the reconstructed complete historical worktree before recognizing residue.
pub struct ResidueProof {
    owner: String,
    source_version: String,
    files: BTreeMap<String, (String, usize)>,
}
impl ResidueProof {
    /// Use the sole native index/outline freshness boundary, scoped to this owner's files.
    pub fn capture(
        policy: &crate::Policy,
        source: &Path,
        base: Option<&Path>,
        owner: &str,
    ) -> Result<Self> {
        require(base.is_none(), "unsupported old sparse adapter; use native Git sparse checkout")?;
        Self::capture_source(policy, source, owner)
    }
    fn capture_source(policy: &crate::Policy, source: &Path, owner: &str) -> Result<Self> {
        require(crate::uuid(owner), "invalid residue owner")?;
        let source = fs::canonicalize(source)?;
        let query = |tx: &rusqlite::Transaction<'_>, _: &str| {
            let mut files = BTreeMap::new();
            for name in [
                "outline.lean",
                "outline.full.lean",
                "outline.lean.meta.json",
                "outline.full.lean.meta.json",
            ] {
                let relative = format!("nodes/{owner}/{name}");
                let (hash, length): (String, i64) = tx.query_row(
                    "SELECT bytes_hash,size FROM outline_cache WHERE path=?1 AND node_id=?2",
                    rusqlite::params![relative, owner],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let length = usize::try_from(length)?;
                let path = source.join(&relative);
                directory(path.parent().ok_or("missing artifact parent")?)?;
                let before = file_identity(&path)?.ok_or("missing owner outline artifact")?;
                let bytes = crate::read_regular(&path)?;
                require(
                    bytes.len() == length && sha256(&bytes) == hash,
                    "unverified residue source bytes",
                )?;
                require(file_identity(&path)? == Some(before), "changed residue source")?;
                if name.ends_with(".lean") {
                    let document = decode_lean(std::str::from_utf8(&bytes)?)?;
                    require(document["status"] != "invalid_draft", "unverified residue source")?;
                }
                files.insert(name.to_owned(), (hash, length));
            }
            Ok(files)
        };
        let (source_version, files) = crate::index::with_current(policy, &source, query)?;
        Ok(Self {
            owner: owner.to_owned(),
            source_version,
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
