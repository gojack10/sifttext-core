//! Compile all consumers of a new private generation from one bounded set of
//! Git-verified inputs. No facts or postings survive from a previous request.
use super::*;
use crate::{node_links::Shortcut, outline_rows::Node, source_error::context};
use std::collections::HashMap;
#[path = "page_image.rs"]
mod page_image;
use page_image::{
    Field::{Blob, Int, Null, Text},
    Image, record,
};
#[path = "posting_segment.rs"]
mod posting_segment;

pub(super) fn compile(
    db: &mut Connection,
    root: &Path,
    snapshot: &Snapshot,
    tree: &str,
    packet: Packet,
    work: &mut Work,
) -> Result<bool> {
    let started = Instant::now();
    let database = PathBuf::from(db.path().ok_or("missing private database file")?);
    db.pragma_update(None, "page_size", page_image::PAGE as i64)?;
    db.pragma_update(None, "synchronous", "EXTRA")?;
    let tx = db.transaction()?;
    // Write the path-ordered ledgers in primary key order. Build their reverse
    // lookup after the load; it is complete before publication.
    tx.execute_batch("CREATE TABLE sources(path TEXT PRIMARY KEY,signature BLOB NOT NULL,content_hash TEXT) WITHOUT ROWID;
        CREATE TABLE git_sources(path TEXT PRIMARY KEY,signature BLOB NOT NULL,content_hash TEXT,oid TEXT NOT NULL) WITHOUT ROWID;")?;
    setup(&tx, true)?;
    crate::leviathan_search::initialize_compiled(&tx)?;
    // The empty FTS table's tokenizer and positional format are retained;
    // it needs no per-token SQL updates to read the finished segments.

    let mut source_rows = Vec::new();
    let mut records = Vec::new();
    let mut outlines = Vec::new();
    let mut ledger = Builder {
        db: &tx,
        packed: Some(&mut source_rows),
        version: source_version::start(),
        count: 0,
    };
    let dir = Source::directory();
    ledger.add("nodes", &dir, None)?;
    let (mut owners, mut roots) = (BTreeSet::<String>::new(), BTreeSet::<String>::new());
    let mut nodes = Vec::new();
    let mut positions = HashMap::new();
    let mut children = BTreeMap::<String, Vec<String>>::new();
    let mut refs = BTreeSet::<(String, String)>::new();
    let mut body_refs = BTreeSet::<(String, String)>::new();
    let mut in_trees = false;
    for inode_source::Entry {
        path: relative,
        kind,
        oid,
        bytes,
    } in packet
    {
        if kind == 0 {
            continue;
        }
        let path = relative.to_str().ok_or("invalid path")?;
        let source = Source::from_git(&relative, kind, oid, bytes.len() as u64)?;
        if let Some(target) = &source.target {
            require(
                target.as_os_str().as_encoded_bytes() == bytes.as_slice(),
                "unsafe Git relationship target",
            )?;
        }
        let parts: Vec<_> = path.split('/').collect();
        match parts[..] {
            ["nodes", id, ..] if parts.len() >= 3 && owners.insert(id.to_owned()) => {
                ledger.add(&format!("nodes/{id}"), &dir, None)?;
                ledger.add(&format!("nodes/{id}/children"), &dir, None)?;
            }
            ["trees", _] if !in_trees => {
                ledger.add("trees", &dir, None)?;
                in_trees = true;
            }
            _ => (),
        }
        ledger.add(path, &source, Some(&bytes))?;
        if kind == 1 {
            work.files_hashed += 1;
        }
        match &parts[..] {
            ["nodes", id, "node.md"] => {
                let row = context(project_bytes(&bytes, id), &relative, None)?;
                let pos = context(
                    row["position"]
                        .as_u64()
                        .ok_or_else(|| "missing or invalid display position".into()),
                    &relative,
                    None,
                )?;
                positions.insert((*id).to_owned(), pos);
                for link in row["see_also"].as_array().ok_or("invalid see_also")? {
                    refs.insert((
                        (*id).to_owned(),
                        link["target"]
                            .as_str()
                            .ok_or("invalid reference")?
                            .to_owned(),
                    ));
                }
                for target in crate::node_links::body_targets(
                    id,
                    row["content"].as_str().ok_or("missing content")?,
                ) {
                    body_refs.insert(((*id).to_owned(), target));
                }
                nodes.push(row);
                work.nodes_parsed += 1;
            }
            ["nodes", owner, "children", child] if kind == 2 => {
                children
                    .entry((*owner).to_owned())
                    .or_default()
                    .push((*child).to_owned());
            }
            ["trees", id] if kind == 2 => {
                roots.insert((*id).to_owned());
            }
            _ => (),
        }
    }
    require(owners.len() == nodes.len(), "node Markdown is missing")?;
    if !in_trees {
        ledger.add("trees", &dir, None)?;
    }
    let source_count = ledger.count;
    let version = format!("{:x}", ledger.version.finalize());
    let facts = started.elapsed();
    let mut desired = BTreeSet::new();
    for (source, target) in refs.union(&body_refs) {
        if owners.contains(source) && owners.contains(target) {
            desired.extend(Shortcut::reference(source, target));
        }
    }
    drop(refs);
    for (owner, members) in &mut children {
        *members = context(
            crate::ordering::sort(std::mem::take(members), |id| Ok(positions.get(id).copied())),
            &PathBuf::from(format!("nodes/{owner}/node.md")),
            None,
        )?;
        for child in members {
            desired.insert(Shortcut::parent(child, owner));
        }
    }
    drop(owners);
    drop(positions);
    std::thread::scope(|scope| -> Result<()> {
        let image = scope.spawn(move || {
            let t = Instant::now();
            crate::node_links::install_captured(root, &desired)
                .map(|n| (n, t.elapsed()))
                .map_err(|e| e.to_string())
        });
        let mut compact = Vec::with_capacity(nodes.len());
        {
            let mut edge = tx.prepare("INSERT INTO child_links VALUES (?1,?2)")?;
            let mut body_edge = tx.prepare("INSERT INTO body_links VALUES (?1,?2)")?;
            let mut reference = tx.prepare("INSERT OR REPLACE INTO links VALUES (?1,?2,?3,?4)")?;
            let mut mapping =
                tx.prepare("INSERT INTO search_ids(rowid,id,title) VALUES(?1,?2,?3)")?;
            let mut postings = posting_segment::Bulk::new(&tx)?;
            let group = crate::leviathan_search::permitted_group();
            for (src, dst) in body_refs {
                body_edge.execute(params![src, dst])?;
            }
            let mut tree_root = tx.prepare("INSERT INTO tree_roots VALUES (?1)")?;
            for id in &roots {
                tree_root.execute([id])?;
            }
            for (i, mut row) in nodes.into_iter().enumerate() {
                let id = row["id"].as_str().ok_or("missing id")?.to_owned();
                let members = children.remove(&id).unwrap_or_default();
                for child in &members {
                    edge.execute(params![id, child])?;
                }
                let members_json = serde_json::to_string(&members)?;
                row["children"] = json!(members);
                let title = row["name"].as_str().ok_or("missing name")?;
                for (pos, link) in row["see_also"]
                    .as_array()
                    .ok_or("missing references")?
                    .iter()
                    .enumerate()
                {
                    reference.execute(params![
                        id,
                        link["target"].as_str(),
                        link["reason"].as_str(),
                        pos as i64
                    ])?;
                }
                let packed = record(&[
                    Text(&id),
                    Text(title),
                    Text(row["micro_summary"].as_str().ok_or("missing summary")?),
                    Text(row["scope"].as_str().unwrap_or("")),
                    Text(""),
                    Text(row["status"].as_str().ok_or("missing status")?),
                    Text(&members_json),
                    Text(&serde_json::to_string(&row)?),
                ]);
                records.push((id.clone(), packed));
                let body = super::super::super::project_body(&row)?;
                let rowid = i64::try_from(i + 1)?;
                mapping.execute(params![rowid, id, title.trim()])?;
                postings.document(rowid, [title.trim(), &body, &group, &group])?;
                row.as_object_mut()
                    .ok_or("malformed node")?
                    .remove("content");
                compact.push(serde_json::from_value::<Node>(row)?);
            }
            postings.finish()?;
        }
        tx.execute_batch(super::super::super::git::GIT_SCHEMA)?;
        let t = Instant::now();
        work.outline_rows_written = crate::outlines::write_compiled(
            &tx,
            &roots,
            &compact,
            |id, tree, pre, parent, depth, desc, fields, input, tokens| {
                let packed = record(&[
                    Text(id),
                    Text(tree),
                    Int(pre),
                    parent.map_or(Null, Text),
                    Int(depth),
                    Int(desc),
                    Text(fields),
                    Text(input),
                    Int(tokens),
                ]);
                outlines.push((
                    id.to_owned(),
                    tree.to_owned(),
                    pre,
                    parent.map(str::to_owned),
                    depth,
                    packed,
                ));
                Ok(())
            },
        )?;
        work.outline_refresh_us = t.elapsed().as_micros();
        let (written, dur) = image.join().map_err(|_| "navigation worker failed")??;
        work.node_link_writes = written;
        work.node_link_refresh_us = dur.as_micros();
        Ok(())
    })?;
    if !snapshot.unchanged()? {
        return Err(super::super::super::Refusal::changed_source().into());
    }
    work.source_rows_examined = source_count as u64;
    work.source_rows_changed = source_count as u64;
    for (key, val) in [
        ("git_tree", tree),
        ("source_count", &source_count.to_string()),
        ("source_version_algorithm", source_version::ALGORITHM),
        ("processing", processing_identity()),
        ("git_working", &snapshot.identity),
        ("generation", "1"),
        ("node_links_ready", "1"),
    ] {
        put(&tx, key, val)?;
    }
    tx.commit()?;
    // The large roots are empty in the SQLite-built catalog. Close its last
    // handle, append sorted pages to that same private inode, and reopen it.
    let layout = db
        .prepare("SELECT name,rootpage FROM sqlite_schema WHERE rootpage>0")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    std::mem::replace(db, Connection::open_in_memory()?)
        .close()
        .map_err(|(_, e)| e)?;
    let mut image = Image::open(&database, layout)?;
    source_rows.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    image.index(
        "sources",
        source_rows.iter().map(|r| {
            record(&[
                Text(&r.path),
                Blob(&r.signature),
                r.hash.as_deref().map_or(Null, Text),
            ])
        }),
    )?;
    image.index(
        "git_sources",
        source_rows.iter().filter_map(|r| {
            Some(record(&[
                Text(&r.path),
                Blob(&r.signature),
                r.hash.as_deref().map_or(Null, Text),
                Text(r.oid.as_deref()?),
            ]))
        }),
    )?;
    source_rows.retain(|r| r.oid.is_some() && r.hash.is_some());
    source_rows
        .sort_unstable_by(|a, b| (&a.oid, &a.hash, &a.path).cmp(&(&b.oid, &b.hash, &b.path)));
    image.index(
        "git_sources_known_hash",
        source_rows.into_iter().map(|r| {
            record(&[
                Text(r.oid.as_deref().unwrap()),
                Text(r.hash.as_deref().unwrap()),
                Text(&r.path),
            ])
        }),
    )?;
    // The capture supplies nodes in component/UUID order, also SQLite BINARY order here.
    require(
        records.windows(2).all(|w| w[0].0 < w[1].0),
        "unordered node capture",
    )?;
    image.index(
        "sqlite_autoindex_nodes_1",
        records
            .iter()
            .enumerate()
            .map(|(i, (id, _))| record(&[Text(id), Int(i as u64 + 1)])),
    )?;
    image.table(
        "nodes",
        records
            .into_iter()
            .enumerate()
            .map(|(i, (_, r))| (i as u64 + 1, r)),
    )?;
    let mut order: Vec<_> = (0..outlines.len()).collect();
    order.sort_unstable_by(|&a, &b| outlines[a].0.cmp(&outlines[b].0));
    image.index(
        "sqlite_autoindex_outline_rows_1",
        order
            .iter()
            .map(|&i| record(&[Text(&outlines[i].0), Int(i as u64 + 1)])),
    )?;
    order.sort_unstable_by(|&a, &b| {
        (&outlines[a].1, outlines[a].2).cmp(&(&outlines[b].1, outlines[b].2))
    });
    image.index(
        "outline_preorder",
        order
            .iter()
            .map(|&i| record(&[Text(&outlines[i].1), Int(outlines[i].2), Int(i as u64 + 1)])),
    )?;
    image.index(
        "outline_deep_rows",
        order.iter().filter(|&&i| outlines[i].4 >= 1000).map(|&i| {
            record(&[
                Text(&outlines[i].1),
                Int(outlines[i].2),
                Int(outlines[i].4),
                Int(i as u64 + 1),
            ])
        }),
    )?;
    order.sort_unstable_by(|&a, &b| {
        (&outlines[a].3, outlines[a].2, a).cmp(&(&outlines[b].3, outlines[b].2, b))
    });
    image.index(
        "outline_children",
        order.iter().map(|&i| {
            record(&[
                outlines[i].3.as_deref().map_or(Null, Text),
                Int(outlines[i].2),
                Int(i as u64 + 1),
            ])
        }),
    )?;
    image.table(
        "outline_rows",
        outlines
            .into_iter()
            .enumerate()
            .map(|(i, r)| (i as u64 + 1, r.5)),
    )?;
    image.finish()?;
    *db = open_cache(database.parent().ok_or("missing database parent")?, false)?;
    if !snapshot.unchanged()? {
        return Err(super::super::super::Refusal::changed_source().into());
    }
    let mode: String = db.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    require(mode == "wal", "could not enable WAL before publication")?;
    db.pragma_update(None, "synchronous", "FULL")?;
    let tx = db.transaction()?;
    put(&tx, "schema", SCHEMA)?;
    put(&tx, "version", &version)?;
    put(&tx, "artifacts_ready", "1")?;
    tx.commit()?;
    if std::env::var_os("SIFTTEXT_RETRIEVAL_TRACE").is_some() {
        eprintln!(
            "index-phase compiled nodes={} facts_us={} total_us={}",
            work.nodes_parsed,
            facts.as_micros(),
            started.elapsed().as_micros()
        );
    }
    Ok(true)
}
