//! Build the first private generation from an unchanged Git tree. The incremental
//! path needs old and expected source tables and projected rows. Here there is
//! no prior generation: hash and project each object as it enters the database.
use super::*;
use crate::source_version::{self, Part};

struct Builder<'a> {
    db: &'a Transaction<'a>,
    version: Sha256,
    count: usize,
}

impl Builder<'_> {
    fn add(&mut self, path: &str, source: &Source, content: Option<&[u8]>) -> Result<()> {
        let digest = content.map(Sha256::digest);
        if crate::coherent_capture::authored_path(Path::new(path)) {
            let part = match (source.kind, digest.as_ref()) {
                (0, None) => Part::Directory,
                (1, Some(hash)) => Part::File {
                    len: source.size,
                    hash: hash.as_ref(),
                },
                (2, Some(hash)) => Part::Link {
                    len: source.size,
                    hash: hash.as_ref(),
                },
                _ => return Err("invalid source kind".into()),
            };
            source_version::add(&mut self.version, Path::new(path), part)?;
        }
        let hash = digest.map(|bytes| format!("{bytes:x}"));
        let signature = source.encode()?;
        self.db
            .prepare_cached("INSERT INTO sources VALUES (?1,?2,?3)")?
            .execute(params![path, signature, hash])?;
        if let Some(blob) = &source.blob {
            self.db
                .prepare_cached("INSERT INTO git_sources VALUES (?1,?2,?3,?4)")?
                .execute(params![path, signature, hash, blob])?;
        }
        self.count += 1;
        Ok(())
    }
}

pub(super) fn refresh(
    connection: &mut Connection,
    root: &Path,
    snapshot: &Snapshot,
    tree: &str,
    node_links: bool,
    work: &mut Work,
) -> Result<Option<bool>> {
    use crate::source_error::context;
    let ledger = crate::git_snapshot::GitReader::inventory(root, tree)?;
    // Keep the position and child vectors bounded. Larger inputs retain the
    // existing disk-staged route. No Markdown bodies accumulate in memory.
    let entries: i64 = ledger
        .connection
        .query_row("SELECT count(*) FROM entries", [], |r| r.get(0))?;
    if entries > 131072 {
        return Ok(None);
    }
    setup(connection, true)?;
    connection.execute_batch(super::super::git::GIT_SCHEMA)?;
    connection.pragma_update(None, "synchronous", "EXTRA")?;
    let tx = connection.transaction()?;
    crate::leviathan_search::initialize_incremental(&tx)?;
    let mut build = Builder {
        db: &tx,
        version: source_version::start(),
        count: 0,
    };
    let dir = Source::directory();
    build.add("nodes", &dir, None)?;
    let mut trees = false;
    let mut owner = String::new();
    let mut positions = BTreeMap::new();
    let mut children = BTreeMap::<String, Vec<String>>::new();
    let mut objects = Objects::new(root);
    let mut statement = ledger
        .connection
        .prepare("SELECT path,kind,oid,len FROM entries WHERE kind<>0 ORDER BY ordering")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let path: String = row.get(0)?;
        let source = Source::from_git(
            Path::new(&path),
            row.get(1)?,
            row.get(2)?,
            u64::try_from(row.get::<_, i64>(3)?)?,
        )?;
        let parts: Vec<_> = path.split('/').collect();
        if parts[0] == "trees" && !trees {
            build.add("trees", &dir, None)?;
            trees = true;
        }
        if parts[0] == "nodes" && parts.len() > 2 && parts[1] != owner {
            if !owner.is_empty() {
                require(positions.contains_key(&owner), "node Markdown is missing")?;
            }
            owner = parts[1].to_owned();
            build.add(&format!("nodes/{owner}"), &dir, None)?;
            build.add(&format!("nodes/{owner}/children"), &dir, None)?;
        }
        let bytes = objects.source_bytes(Path::new(&path), &source)?;
        if source.kind == 2 {
            require(
                source
                    .target
                    .as_ref()
                    .map(|p| p.as_os_str().as_encoded_bytes())
                    == Some(bytes.as_slice()),
                "unsafe Git relationship target",
            )?;
        }
        build.add(&path, &source, Some(&bytes))?;
        match &parts[..] {
            ["trees", id] if source.kind == 2 => {
                tx.prepare_cached("INSERT INTO tree_roots VALUES (?1)")?
                    .execute([id])?;
            }
            ["nodes", parent, "children", child] if source.kind == 2 => {
                children
                    .entry((*parent).into())
                    .or_default()
                    .push((*child).into());
                tx.prepare_cached("INSERT INTO child_links VALUES (?1,?2)")?
                    .execute(params![parent, child])?;
            }
            ["nodes", id, "node.md"] => {
                let mut row = context(project_bytes(&bytes, id), Path::new(&path), None)?;
                let pos = context(
                    row["position"]
                        .as_u64()
                        .ok_or_else(|| "missing or invalid display position".into()),
                    Path::new(&path),
                    None,
                )?;
                positions.insert((*id).to_owned(), pos);
                row["children"] = json!([]);
                // Validate the projection now; the append scan can run while ext4 installs the links.
                super::super::project_body(&row)?;
                write_record(&tx, &row, None)?;
                crate::node_links::index_body(
                    &tx,
                    id,
                    row["content"].as_str().ok_or("missing content")?,
                )?;
                work.nodes_parsed += 1;
            }
            _ => (),
        }
        if source.kind == 1 {
            work.files_hashed += 1;
        }
    }
    if !owner.is_empty() {
        require(positions.contains_key(&owner), "node Markdown is missing")?;
    }
    if !trees {
        build.add("trees", &dir, None)?;
    }
    drop(rows);
    drop(statement);
    drop(ledger);
    drop(objects);
    for (id, members) in children {
        let sorted = context(
            crate::ordering::sort(members, |child| Ok(positions.get(child).copied())),
            &PathBuf::from(format!("nodes/{id}/node.md")),
            None,
        )?;
        tx.prepare_cached("UPDATE nodes SET children=?2, record=json_set(record,'$.children',json(?2)) WHERE id=?1")?
            .execute(params![id,serde_json::to_string(&sorted)?])?;
    }
    drop(positions);
    let outlines = Instant::now();
    work.outline_rows_written =
        crate::outlines::refresh_indexed_from(&tx, root, &BTreeSet::new(), true)?;
    work.outline_refresh_us += outlines.elapsed().as_micros();
    let postings = || -> Result<()> {
        let mut scan = tx.prepare("SELECT id,name,micro_summary,scope,json_extract(record,'$.content') FROM nodes ORDER BY id")?;
        let mut rows = scan.query([])?;
        while let Some(row) = rows.next()? {
            let field = |column| -> Result<&str> { Ok(row.get_ref(column)?.as_str()?) };
            let doc = crate::search_document::project(crate::search_document::Fields {
                name: field(1)?,
                micro_summary: field(2)?,
                scope: field(3)?,
                content: field(4)?,
            })?;
            crate::leviathan_search::insert_new(&tx, field(0)?, doc.title, &doc.body)?;
        }
        Ok(())
    };
    if node_links {
        let start = Instant::now();
        let desired = crate::node_links::desired(&tx)?;
        work.node_link_writes = std::thread::scope(|scope| -> Result<u64> {
            let links = scope.spawn(move || {
                crate::node_links::install(root, &desired).map_err(|error| {
                    error
                        .downcast::<std::io::Error>()
                        .map(|e| *e)
                        .map_err(|e| e.to_string())
                })
            });
            let search = postings();
            let written = links
                .join()
                .map_err(|_| "navigation worker failed")?
                .map_err(|error| -> Box<dyn std::error::Error> {
                    match error {
                        Ok(io) => Box::new(io),
                        Err(text) => text.into(),
                    }
                })?;
            search?;
            Ok(written)
        })?;
        work.node_link_refresh_us += start.elapsed().as_micros();
    } else {
        postings()?;
    }
    if !snapshot.unchanged()? {
        return Ok(Some(false));
    }
    let version = format!("{:x}", build.version.finalize());
    put(&tx, "git_tree", tree)?;
    put(&tx, "source_count", &build.count.to_string())?;
    work.source_rows_examined = build.count as u64;
    work.source_rows_changed = build.count as u64;
    put(&tx, "source_version_algorithm", source_version::ALGORITHM)?;
    put(&tx, "processing", processing_identity())?;
    put(&tx, "git_working", &snapshot.identity)?;
    put(&tx, "generation", "1")?;
    put(&tx, "node_links_ready", if node_links { "1" } else { "0" })?;
    tx.commit()?;
    let mode: String = connection.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    require(
        mode == "wal",
        "could not enable WAL before index publication",
    )?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    let tx = connection.transaction()?;
    put(&tx, "schema", SCHEMA)?;
    put(&tx, "version", &version)?;
    put(&tx, "artifacts_ready", "1")?;
    tx.commit()?;
    Ok(Some(true))
}
