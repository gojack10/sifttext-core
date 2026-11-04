//! Ordering-only import, bound to immutable original and converted Git trees.
//! Rust owns byte decoding, exact path custody and complete batch coverage.
use crate::{
    Policy, Result,
    git_snapshot::{Batch, GitReader},
    local, ordering, require,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

struct Entry {
    kind: u8,
    mode: String,
    oid: String,
}

struct Snapshot {
    entries: BTreeMap<String, Entry>,
    texts: BTreeMap<String, String>,
    children: BTreeMap<String, BTreeSet<String>>,
    hints: BTreeMap<String, String>,
}

impl Snapshot {
    fn read(repo: &Path, revision: &str) -> Result<Self> {
        let tree = local::reader_git(
            repo,
            &["rev-parse", "--verify", &format!("{revision}^{{tree}}")],
        )?;
        let ledger = GitReader::inventory(repo, &tree)?;
        let mut statement = ledger
            .connection
            .prepare("SELECT path,kind,mode,oid,len FROM entries ORDER BY path")?;
        let mut rows = statement.query([])?;
        let mut batch = Batch::new(repo)?;
        let mut result = Self {
            entries: BTreeMap::new(),
            texts: BTreeMap::new(),
            children: BTreeMap::new(),
            hints: BTreeMap::new(),
        };
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let kind: u8 = row.get(1)?;
            let mode: String = row.get(2)?;
            let oid: String = row.get(3)?;
            let len = u64::try_from(row.get::<_, i64>(4)?)?;
            let parts: Vec<_> = path.split('/').collect();
            match parts.as_slice() {
                ["nodes", id, "node.md"] => {
                    require(kind == 1, "unsafe migration node file")?;
                    let text = String::from_utf8(batch.bytes(&oid, len)?)?;
                    require(
                        crate::parse_markdown(&text)?.id == *id,
                        "migration node identity mismatch",
                    )?;
                    ordering::position(&text)?;
                    result.texts.insert((*id).to_owned(), text);
                }
                ["nodes", id, "children"] => {
                    require(kind == 0, "unsafe migration children directory")?;
                    result.children.insert((*id).to_owned(), BTreeSet::new());
                }
                ["nodes", id, "children", child] if *child != ".keep" => {
                    require(kind == 2 && crate::uuid(child), "invalid migration child")?;
                    require(
                        batch.bytes(&oid, len)? == format!("../../{child}").as_bytes(),
                        "unsafe migration child link",
                    )?;
                    result
                        .children
                        .get_mut(*id)
                        .ok_or("missing migration children directory")?
                        .insert((*child).to_owned());
                }
                ["nodes", id, "children.order"] => {
                    require(kind == 1, "unsafe legacy child list")?;
                    let text = String::from_utf8(batch.bytes(&oid, len)?)?;
                    result.hints.insert((*id).to_owned(), text);
                }
                _ => (),
            }
            result.entries.insert(path, Entry { kind, mode, oid });
        }
        require(
            result.children.keys().eq(result.texts.keys()),
            "migration children/node inventory mismatch",
        )?;
        let mut placed = BTreeSet::new();
        for members in result.children.values() {
            for child in members {
                require(result.texts.contains_key(child), "unknown migration child")?;
                require(placed.insert(child), "multiple migration parents")?;
            }
        }
        require(
            result.hints.keys().all(|id| result.texts.contains_key(id)),
            "unknown migration list owner",
        )?;
        Ok(result)
    }
}

/// Always derive from the original commit, including after an interrupted write
/// or list removal. A half-written draft never becomes the next source of truth.
pub(crate) fn convert(repo: &Path, draft: &Path, base: &str) -> Result<BTreeSet<String>> {
    let before = Snapshot::read(repo, base)?;
    crate::real_dir(&draft.join("nodes"))?;
    // Lists may already be absent after interruption. Preserve any divergent
    // draft bytes instead of deleting an owner's attempted repair.
    for (parent, hints) in &before.hints {
        let path = draft.join("nodes").join(parent).join("children.order");
        crate::real_dir(path.parent().ok_or("missing migration node directory")?)?;
        match fs::symlink_metadata(&path) {
            Ok(_) => require(
                crate::read_regular(&path)? == hints.as_bytes(),
                "migration child list was edited; retain the draft",
            )?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    let mut ranks = BTreeMap::new();
    for (parent, hints) in &before.hints {
        let mut remaining = before.children[parent].clone();
        let mut order = Vec::new();
        for child in hints.lines() {
            if remaining.remove(child) {
                order.push(child.to_owned());
            }
        }
        order.extend(remaining);
        for (index, child) in order.into_iter().enumerate() {
            let rank = u64::try_from(index)?
                .checked_add(1)
                .and_then(|n| n.checked_mul(1024))
                .ok_or("migration position overflow")?;
            ranks.insert(child, rank);
        }
    }
    for (id, original) in &before.texts {
        let directory = draft.join("nodes").join(id);
        crate::real_dir(&directory)?;
        let path = directory.join("node.md");
        let current = String::from_utf8(crate::read_regular(&path)?)?;
        require(
            ordering::without_position(&crate::migration_size::without_record(&current))
                == ordering::without_position(&crate::migration_size::without_record(original)),
            "migration draft contains authored edits; retain the draft",
        )?;
        let rank = ranks
            .get(id)
            .copied()
            .or(ordering::position(original)?)
            .unwrap_or(0);
        if ordering::position(&current)? != Some(rank) {
            let converted = ordering::with_position(original, rank)?;
            let converted = crate::migration_size::attach(original, &converted, false)?;
            fs::write(path, converted)?;
        }
    }
    // All positions are written before removing any legacy list.
    for parent in before.hints.keys() {
        let path = draft.join("nodes").join(parent).join("children.order");
        match fs::remove_file(path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(before.texts.into_keys().collect())
}

/// Inspect actual committed output, rather than trusting the writer's rank map.
pub(crate) fn check(policy: &Policy, repo: &Path, base: &str, converted: &str) -> Result<()> {
    let before = Snapshot::read(repo, base)?;
    let after = Snapshot::read(repo, converted)?;
    require(
        after.hints.is_empty(),
        "legacy lists remain after migration",
    )?;
    let removed: BTreeSet<_> = before
        .hints
        .keys()
        .map(|id| format!("nodes/{id}/children.order"))
        .collect();
    let nodes: BTreeSet<_> = before
        .texts
        .keys()
        .map(|id| format!("nodes/{id}/node.md"))
        .collect();
    let expected: BTreeSet<_> = before
        .entries
        .keys()
        .filter(|path| !removed.contains(*path))
        .collect();
    require(
        expected.into_iter().eq(after.entries.keys()),
        "ordering migration changed authored paths",
    )?;
    for (path, entry) in &after.entries {
        let old = &before.entries[path];
        require(
            entry.kind == old.kind && entry.mode == old.mode,
            "ordering migration changed entry mode",
        )?;
        if entry.kind != 0 && !nodes.contains(path) {
            require(
                entry.oid == old.oid,
                "ordering migration changed a relationship or authored file",
            )?;
        }
    }
    for (id, text) in &before.texts {
        require(
            ordering::without_position(&crate::migration_size::without_record(text))
                == ordering::without_position(&crate::migration_size::without_record(&after.texts[id])),
            "ordering migration changed non-position bytes",
        )?;
        let output = &after.texts[id];
        if !crate::migration_size::has_record(text)? {
            require(crate::migration_size::attach(text,
                &crate::migration_size::without_record(output), false)? == *output,
                "ordering migration size evidence changed")?;
        }
        require(
            ordering::position(&after.texts[id])?.is_some(),
            "migration output lacks a position",
        )?;
    }
    let mut ranked = BTreeSet::new();
    for (parent, hints) in &before.hints {
        let table: Vec<_> = before.children[parent].iter().collect();
        let ordinals: BTreeMap<_, _> = table
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i as u64 + 1))
            .collect();
        // Project only ghosts away. Bend receives every matching occurrence.
        let projected: Vec<_> = hints
            .lines()
            .filter_map(|id| ordinals.get(id).copied())
            .collect();
        let members: Vec<_> = after.children[parent].iter().collect();
        check_batches(policy, 1, &table, &members, &projected, &before, &after)?;
        ranked.extend(table.into_iter().cloned());
    }
    let table: Vec<_> = before
        .texts
        .keys()
        .filter(|id| !ranked.contains(*id))
        .collect();
    let members: Vec<_> = after
        .texts
        .keys()
        .filter(|id| !ranked.contains(*id))
        .collect();
    check_batches(policy, 0, &table, &members, &[], &before, &after)
}

fn check_batches(
    policy: &Policy,
    mode: u64,
    table: &[&String],
    members: &[&String],
    hints: &[u64],
    before: &Snapshot,
    after: &Snapshot,
) -> Result<()> {
    // Four 16-bit limbs encode every logical u64, including counts and ordinals.
    // At most 28,339 matching hints fit in a bounded legacy file. With 4,096
    // rows of three values and seven header values, requests need < 975,287 bytes.
    let ordinal: BTreeMap<_, _> = table
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, i as u64 + 1))
        .collect();
    let mut start = 0usize;
    loop {
        let end = start.saturating_add(4096).min(members.len());
        let mut values = vec![
            1,
            mode,
            table.len() as u64,
            members.len() as u64,
            start as u64,
            (end - start) as u64,
            hints.len() as u64,
        ];
        values.extend_from_slice(hints);
        for id in &members[start..end] {
            values.extend([
                ordinal.get(id).copied().unwrap_or(0),
                before
                    .texts
                    .get(*id)
                    .map(|text| ordering::position(text))
                    .transpose()?
                    .flatten()
                    .unwrap_or(0),
                ordering::position(&after.texts[*id])?.ok_or("missing migration position")?,
            ]);
        }
        let words: Vec<usize> = values
            .into_iter()
            .flat_map(|value| [48, 32, 16, 0].map(|shift| ((value >> shift) & 0xffff) as usize))
            .collect();
        policy.order_migration(&words, table.len(), hints.len())?;
        if end == members.len() {
            break;
        }
        start = end;
    }
    Ok(())
}
