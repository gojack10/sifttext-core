//! Per-node display positions. Relationships remain ordinary Git symlinks.
//! Legacy parent lists are read until an explicitly requested migration.
use crate::{Result, require};
use std::{collections::BTreeSet, fs, path::Path};

pub fn position(text: &str) -> Result<Option<u64>> {
    crate::markdown(text)?
        .fields
        .get("position")
        .map(|raw| serde_json::from_str(raw).map_err(Into::into))
        .transpose()
}

pub fn sort(
    members: impl IntoIterator<Item = String>,
    mut position_of: impl FnMut(&str) -> Result<Option<u64>>,
) -> Result<Vec<String>> {
    let mut ranked = Vec::new();
    for id in members {
        require(crate::uuid(&id), "invalid child UUID")?;
        let rank = position_of(&id)?
            .ok_or("child lacks position; migrate the legacy parent list first")?;
        ranked.push((rank, id));
    }
    ranked.sort(); // UUID is the deterministic tie-break for concurrent appends.
    Ok(ranked.into_iter().map(|(_, id)| id).collect())
}

pub fn children(home: &Path, id: &str) -> Result<Vec<String>> {
    require(crate::uuid(id), "invalid parent UUID")?;
    let dir = home.join("nodes").join(id);
    match fs::symlink_metadata(dir.join("children.order")) {
        Ok(_) => {
            let bytes = crate::read_regular(&dir.join("children.order"))?;
            let result: Vec<_> = std::str::from_utf8(&bytes)?
                .lines()
                .map(str::to_owned)
                .collect();
            require(
                result.iter().all(|id| crate::uuid(id)),
                "invalid legacy child UUID",
            )?;
            Ok(result)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            require(
                position(std::str::from_utf8(&crate::read_regular(
                    &dir.join("node.md"),
                )?)?)?
                .is_some(),
                "missing children.order or per-node position",
            )?;
            crate::real_dir(&dir.join("children"))?;
            let mut members = BTreeSet::new();
            for entry in fs::read_dir(dir.join("children"))? {
                let entry = entry?;
                if entry.file_name() == ".keep" {
                    continue;
                }
                let child = entry
                    .file_name()
                    .into_string()
                    .map_err(|_| "non-UTF8 child")?;
                require(crate::uuid(&child), "invalid child UUID")?;
                require(
                    fs::read_link(entry.path())? == Path::new(&format!("../../{child}")),
                    "unsafe child link",
                )?;
                members.insert(child);
            }
            sort(members, |child| {
                position(std::str::from_utf8(&crate::read_regular(
                    &home.join("nodes").join(child).join("node.md"),
                )?)?)
            })
        }
        Err(error) => Err(error.into()),
    }
}

/// Change only the front-matter field, preserving every other authored byte.
pub fn with_position(text: &str, value: u64) -> Result<String> {
    crate::parse_markdown(text)?;
    let mut result = String::new();
    let mut fence = 0;
    let mut written = false;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            fence += 1;
            if fence == 2 && !written {
                result.push_str(&format!("position: {value}\n"));
            }
        }
        if fence == 1
            && line
                .split_once(':')
                .is_some_and(|(key, _)| key.trim() == "position")
        {
            result.push_str(&format!("position: {value}\n"));
            written = true;
        } else {
            result.push_str(line);
        }
    }
    Ok(result)
}

/// Display positions are structural metadata, not a new semantic checker input.
pub fn without_position(text: &str) -> String {
    let mut fence = 0;
    text.split_inclusive('\n')
        .filter(|line| {
            if line.trim_end_matches(['\r', '\n']) == "---" {
                fence += 1;
            }
            !(fence == 1
                && line
                    .split_once(':')
                    .is_some_and(|(key, _)| key.trim() == "position"))
        })
        .collect()
}

/// Mechanical migration in an allocated private draft, never an accepted home.
/// The caller commits and submits through ordinary native publication.
pub fn migrate_draft(path: &Path) -> Result<serde_json::Value> {
    migrate(path, false)
}

pub fn restore_legacy_draft(path: &Path) -> Result<serde_json::Value> {
    migrate(path, true)
}

fn migrate(path: &Path, legacy: bool) -> Result<serde_json::Value> {
    require(
        fs::symlink_metadata(path.join(".git"))?.is_file(),
        "migrate in a private Git worktree",
    )?;
    if crate::local::sparse_checkout(path)? {
        crate::local::reader_git(path, &["sparse-checkout", "disable"])?;
    }
    let store = crate::Store::read(path)?;
    if legacy {
        for (id, children) in &store.children {
            fs::write(
                path.join("nodes").join(id).join("children.order"),
                children
                    .iter()
                    .map(|child| format!("{child}\n"))
                    .collect::<String>(),
            )?;
        }
        return Ok(
            serde_json::json!({"status":"legacy_draft_restored", "nodes":store.nodes.len(),
            "next":"Commit and submit with the new Core before selecting the previous binary. No knowledge history is reset."}),
        );
    }
    let mut ranks = std::collections::BTreeMap::new();
    for id in &store.roots {
        ranks.insert(id.clone(), 0);
    }
    for children in store.children.values() {
        for (index, child) in children.iter().enumerate() {
            ranks.insert(child.clone(), (index as u64 + 1) * 1024);
        }
    }
    let mut converted = 0;
    for id in store.nodes.keys() {
        let dir = path.join("nodes").join(id);
        if !dir.join("children.order").try_exists()? {
            continue;
        }
        let file = dir.join("node.md");
        let text = String::from_utf8(crate::read_regular(&file)?)?;
        let rank = *ranks.get(id).ok_or("unplaced node")?;
        fs::write(&file, with_position(&text, rank)?)?;
        converted += 1;
    }
    // All positions exist before removing any list. Interrupted drafts are retained.
    for id in store.nodes.keys() {
        let file = path.join("nodes").join(id).join("children.order");
        if file.try_exists()? {
            fs::remove_file(file)?;
        }
    }
    Ok(
        serde_json::json!({"status":"draft_migrated", "nodes":converted,
        "next":"Commit this draft and submit with its original home and base. Upgrade every writer before acceptance."}),
    )
}
