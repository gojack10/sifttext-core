//! Checked packet of ordinary authored ext4 entries. Only a sealed checkout
//! with an independently matching Git index may use it in place of status.
use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

pub(crate) struct Entry {
    pub path: PathBuf,
    pub kind: u8,
    pub oid: String,
    pub bytes: Vec<u8>,
}
pub(crate) type Packet = Vec<Entry>;
const READER: &str = "/usr/sbin/sifttext-inode-source";

pub(super) fn capture(root: &Path, tree: &str) -> Option<Packet> {
    if !cfg!(target_os = "linux")
        || root != Path::new("/workspace/knowledge")
        || tree.len() != 40
        || !Path::new(READER).is_file()
    {
        return None;
    }
    let start = Instant::now();
    let result = read(root, tree);
    if std::env::var_os("SIFTTEXT_RETRIEVAL_TRACE").is_some() {
        eprintln!(
            "index-phase inode_source files={} us={}",
            result
                .as_ref()
                .map_or(0, |rows| rows.iter().filter(|e| e.kind != 0).count()),
            start.elapsed().as_micros()
        );
    }
    result.ok()
}

fn read(root: &Path, tree: &str) -> Result<Packet> {
    let top = crate::local::reader_git(root, &["ls-tree", tree])?;
    let (mut nodes, mut trees) = (None, None);
    for line in top.lines() {
        let (head, name) = line.split_once('\t').ok_or("bad Git tree")?;
        let (entry, oid) = head.rsplit_once(' ').ok_or("bad Git tree entry")?;
        match (entry, name) {
            ("040000 tree", "nodes") => nodes = Some(oid),
            ("040000 tree", "trees") => trees = Some(oid),
            ("100644 blob", ".gitignore") => (),
            _ => return Err("nonstandard checkout layout".into()),
        }
    }
    let roots = [
        nodes.ok_or("missing nodes tree")?,
        trees.ok_or("missing roots tree")?,
    ];
    let mut child = Command::new(READER)
        .args(roots)
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let result = decode(child.stdout.take().ok_or("missing source stream")?);
    if result.is_err() {
        let _ = child.kill();
    }
    require(
        child.wait()?.success(),
        "incomplete or mismatched native Git tree",
    )?;
    let entries = result?;
    for (name, oid) in ["nodes", "trees"].into_iter().zip(roots) {
        require(
            entries
                .iter()
                .any(|e| e.kind == 0 && e.path == Path::new(name) && e.oid == oid),
            "missing verified subtree",
        )?;
    }
    objects_present(root, &entries)?;
    Ok(entries)
}

fn decode(pipe: impl Read) -> Result<Packet> {
    let mut input = BufReader::with_capacity(64 << 10, pipe);
    let mut entries = Vec::new();
    let mut total = 0u64;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if input.by_ref().take(512).read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        require(
            buf.pop() == Some(b'\n') && entries.len() < 131072,
            "oversized source stream",
        )?;
        let mut fields = std::str::from_utf8(&buf)?.split(' ');
        let kind: u8 = fields.next().ok_or("missing kind")?.parse()?;
        let oid = fields.next().ok_or("missing object ID")?;
        let len: u64 = fields.next().ok_or("missing length")?.parse()?;
        let path = PathBuf::from(fields.next().ok_or("missing path")?);
        require(
            fields.next().is_none()
                && oid.len() == 40
                && oid.bytes().all(|c| c.is_ascii_hexdigit()),
            "bad native framing",
        )?;
        let parts: Vec<_> = path.to_str().ok_or("invalid path")?.split('/').collect();
        let known = match (kind, parts.as_slice()) {
            (0, ["nodes" | "trees"]) => true,
            (0, ["nodes", id]) | (0, ["nodes", id, "children"]) => crate::uuid(id),
            (1, ["nodes" | "trees", ".keep"]) => len == 0,
            (1, ["nodes", id, "node.md" | "children.order"]) => crate::uuid(id),
            (1, ["nodes", id, "children", ".keep"]) => crate::uuid(id) && len == 0,
            (2, ["trees", id]) => crate::uuid(id),
            (2, ["nodes", id, "children", child]) => crate::uuid(id) && crate::uuid(child),
            _ => false,
        };
        require(
            known
                && (kind != 0 || len == 0)
                && len <= crate::migration_size::input_limit(&path) as u64,
            "unsupported native input",
        )?;
        total = total.checked_add(len).ok_or("native size overflow")?;
        require(total <= 64 << 20, "oversized native input")?;
        let mut bytes = vec![0; len as usize];
        input.read_exact(&mut bytes)?;
        entries.push(Entry {
            path,
            kind,
            oid: oid.into(),
            bytes,
        });
    }
    // All paths are canonical and have been validated above. The source fold
    // uses component order (children/UUID precedes children.order).
    entries.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    require(
        !entries.windows(2).any(|w| w[0].path == w[1].path),
        "duplicate source path",
    )?;
    Ok(entries)
}

// A complete, byte-for-byte comparison with the staged file list. Never
// infer clean stage 0 entries from a HEAD tree or from the working files.
pub(super) fn stage_matches(root: &Path, rows: &[Entry]) -> Result<bool> {
    let out = crate::checked_publication::run_command(crate::local::reader_command(
        root,
        &["ls-files", "--stage", "-z", "--", "nodes", "trees"],
    ))?;
    if !out.status.success() {
        return Ok(false);
    }
    let mut files: Vec<_> = rows.iter().filter(|e| e.kind != 0).collect();
    files.sort_unstable_by(|a, b| a.path.as_os_str().cmp(b.path.as_os_str()));
    let mut records = out.stdout.split(|b| *b == 0);
    for e in files {
        let Some(s) = records.next() else {
            return Ok(false);
        };
        let Some((info, path)) = std::str::from_utf8(s)?.split_once('\t') else {
            return Ok(false);
        };
        let mut words = info.split(' ');
        if path.as_bytes() != e.path.as_os_str().as_encoded_bytes()
            || words.next() != Some(if e.kind == 1 { "100644" } else { "120000" })
            || words.next() != Some(e.oid.as_str())
            || words.next() != Some("0")
            || words.next().is_some()
        {
            return Ok(false);
        }
    }
    Ok(records.next() == Some(b"".as_slice()) && records.next().is_none())
}

// Git must still possess every object with the captured type and blob size.
fn objects_present(root: &Path, entries: &[Entry]) -> Result<()> {
    let mut objects: Vec<_> = entries
        .iter()
        .map(|e| (e.oid.as_str(), e.kind, e.bytes.len()))
        .collect();
    objects.sort_unstable();
    objects.dedup();
    let mut requests = Vec::with_capacity(41 * objects.len());
    for (oid, _, _) in &objects {
        requests.extend(oid.as_bytes());
        requests.push(b'\n');
    }
    let mut child = crate::local::reader_command(root, &["cat-file", "--batch-check", "--buffer"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().ok_or("missing Git object input")?;
    let output = std::thread::scope(|s| -> Result<_> {
        let send = s.spawn(move || stdin.write_all(&requests));
        let output = child.wait_with_output()?;
        send.join().map_err(|_| "Git request failed")??;
        Ok(output)
    })?;
    require(output.status.success(), "Git object check failed")?;
    let mut lines = std::str::from_utf8(&output.stdout)?.lines();
    for (oid, kind, len) in objects {
        let mut cols = lines.next().ok_or("missing Git object")?.split(' ');
        require(
            cols.next() == Some(oid)
                && cols.next() == Some(if kind == 0 { "tree" } else { "blob" }),
            "wrong Git object",
        )?;
        let size: usize = cols.next().ok_or("missing Git object size")?.parse()?;
        require(
            cols.next().is_none() && (kind == 0 || size == len),
            "wrong Git object size",
        )?;
    }
    require(lines.next().is_none(), "extra Git object status")
}
