//! Per-node display positions. Relationships remain ordinary Git symlinks.
//! Legacy import belongs exclusively to the one-time home format migration.
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
            .ok_or("child lacks a display position")?;
        ranked.push((rank, id));
    }
    ranked.sort(); // UUID is the deterministic tie-break for concurrent appends.
    Ok(ranked.into_iter().map(|(_, id)| id).collect())
}

pub fn children(home: &Path, id: &str) -> Result<Vec<String>> {
    require(crate::uuid(id), "invalid parent UUID")?;
    let dir = home.join("nodes").join(id);
    require(position(std::str::from_utf8(&crate::read_regular(&dir.join("node.md"))?)?)?.is_some(),
        "missing display position")?;
    if !crate::optional_dir(&dir.join("children"))? { return Ok(Vec::new()); }
    let mut members = BTreeSet::new();
    for entry in fs::read_dir(dir.join("children"))? {
        let entry = entry?;
        if entry.file_name() == ".keep" { continue }
        let child = entry.file_name().into_string().map_err(|_| "non-UTF8 child")?;
        require(crate::uuid(&child), "invalid child UUID")?;
        require(fs::read_link(entry.path())? == Path::new(&format!("../../{child}")), "unsafe child link")?;
        members.insert(child);
    }
    sort(members, |child| {
        position(std::str::from_utf8(&crate::read_regular(
            &home.join("nodes").join(child).join("node.md"),
        )?)?)
    })
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
