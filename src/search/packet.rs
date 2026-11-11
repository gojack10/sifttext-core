//! Local CLI source delivery, using the ranking transaction rather than rereading files.
use super::Outcome;
use crate::{Result, lean_render, outlines};
use rusqlite::{OptionalExtension, Transaction};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

impl Outcome {
    pub(super) fn packet(&self, tx: &Transaction<'_>) -> Result<String> {
        // Keep the existing ancestor-warning projection, including explicit source
        // reads for warnings that are too large to inline. Drop processing metadata.
        let mut packet = self.cli_value();
        for key in ["status", "version", "path_template"] {
            packet.as_object_mut().unwrap().remove(key);
        }
        let ids: Vec<_> = self.hits.iter().map(|hit| hit.id.clone()).collect();
        let mut rows = outlines::selected_rows(tx, &ids)?;
        let mut trails = BTreeMap::new();
        let mut chain = Vec::new();
        for row in &self.rows {
            if row["depth"] == 0 { chain.clear(); }
            if row["match"] == true {
                trails.insert(row["id"].as_str().unwrap(), chain.join(" > "));
            } else {
                chain.push(row["name"].as_str().unwrap_or("unavailable"));
            }
        }
        for (position, (row, hit)) in rows.iter_mut().zip(&self.hits).enumerate() {
            row["trail"] = trails.get(hit.id.as_str()).cloned().unwrap_or_default().into();
            if position < 3 {
                let record: String = tx.query_row("SELECT record FROM nodes WHERE id=?1",
                    [&hit.id], |r| r.get(0))?;
                let source: Value = serde_json::from_str(&record)?;
                row["content"] = source["content"].clone();
                if let Some(scope) = source["scope"].as_str().filter(|s| !s.is_empty()) {
                    row["scope"] = scope.into();
                }
                let mut references = Vec::new();
                for link in source["see_also"].as_array().into_iter().flatten() {
                    let id = link["target"].as_str().ok_or("invalid linked node ID")?;
                    let name: Option<String> = tx.query_row("SELECT name FROM nodes WHERE id=?1",
                        [id], |r| r.get(0)).optional()?;
                    references.push(format!("[{}](../{id}/node.md): {}",
                        name.as_deref().unwrap_or(id), link["reason"].as_str().unwrap_or("")));
                }
                if !references.is_empty() { row["see_also"] = references.join("\n").into(); }
            } else if let Some(excerpt) = &hit.matching_excerpt {
                row["matching_excerpt"] = excerpt.as_str().into();
            }
        }
        let children: Vec<String> = if let Some(first) = ids.first() {
            let children: String = tx.query_row("SELECT children FROM nodes WHERE id=?1",
                [first], |r| r.get(0))?;
            serde_json::from_str(&children)?
        } else { Vec::new() };
        let present: BTreeSet<_> = ids.iter().collect();
        for row in &mut rows {
            if children.iter().any(|id| row["id"] == *id) { row["child_of_first"] = true.into(); }
        }
        let missing: Vec<_> = children.into_iter().filter(|id| !present.contains(id)).collect();
        packet["rows"] = rows.into();
        let mut text = lean_render::render(&packet)?;
        if !missing.is_empty() {
            let children = outlines::selected_rows(tx, &missing)?;
            text.push('\n');
            text.push_str(&lean_render::render(&json!({"rows":children}))?
                .replacen("rows[", "children_of_first[", 1));
        }
        Ok(text + "\n")
    }
}
