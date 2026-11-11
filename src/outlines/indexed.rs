//! One navigation row per node, refreshed with the working-view index.
use super::format::{full_outline_header, write_full_outline_row};
use crate::{
    Result, lean_render,
    outline_rows::{Coverage, Forest, Node, PATH_TEMPLATE},
    require, sha256, token_count,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::{collections::BTreeSet, path::Path, sync::OnceLock};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS outline_projection (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1), processing TEXT NOT NULL,
    diagnostic TEXT);
CREATE TABLE IF NOT EXISTS outline_rows (
    id TEXT PRIMARY KEY, tree TEXT NOT NULL, preorder INTEGER NOT NULL,
    parent TEXT, depth INTEGER NOT NULL, descendants INTEGER NOT NULL,
    fields TEXT NOT NULL, input TEXT NOT NULL, subtree_tokens INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS outline_preorder ON outline_rows(tree,preorder);
CREATE INDEX IF NOT EXISTS outline_children ON outline_rows(parent,preorder);
CREATE INDEX IF NOT EXISTS outline_deep_rows ON outline_rows(tree,preorder,depth) WHERE depth>=1000;
CREATE TEMP TABLE IF NOT EXISTS outline_selected (id TEXT PRIMARY KEY);
CREATE TEMP TABLE IF NOT EXISTS outline_changed (id TEXT PRIMARY KEY);
";

fn processing() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        sha256(
            concat!(
                include_str!("indexed.rs"),
                include_str!("format.rs"),
                include_str!("../outline_rows.rs"),
                include_str!("../lean_render.rs"),
                include_str!("../token_count.rs"),
                include_str!("../../Cargo.lock")
            )
            .as_bytes(),
        )
    })
}

pub(crate) fn indexed_current(connection: &Connection) -> Result<bool> {
    if !crate::index::residue::table_exists(connection, "outline_projection")? {
        return Ok(false);
    }
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM outline_projection WHERE singleton=1 AND processing=?1)",
        [processing()],
        |row| row.get(0),
    )?)
}

/// Hydrate only displayed draft row fields; body text is never an outline dependency.
/// Optional fields present with null/non-text values are malformed, not missing metadata.
const COMPACT: &str = "SELECT id,name,status,micro_summary,children,
        CASE WHEN json_type(record)='object'
          AND (json_type(record,'$.warnings') IS NULL OR json_type(record,'$.warnings')='text')
          AND (json_type(record,'$.blocker') IS NULL OR json_type(record,'$.blocker')='text')
          AND (json_type(record,'$.deferred_reason') IS NULL OR json_type(record,'$.deferred_reason')='text')
          AND (json_type(record,'$.discard_reason') IS NULL OR json_type(record,'$.discard_reason')='text')
        THEN json_object('warnings',json_extract(record,'$.warnings'),
            'blocker',json_extract(record,'$.blocker'),'deferred_reason',json_extract(record,'$.deferred_reason'),
            'discard_reason',json_extract(record,'$.discard_reason')) END
        FROM nodes";
fn compact_node(row: &rusqlite::Row<'_>) -> Result<Node> {
    let fields: Option<String> = row.get(5)?;
    let mut fields: Value =
        serde_json::from_str(&fields.ok_or("malformed outline metadata fields")?)?;
    for (key, column) in [("id", 0), ("name", 1), ("status", 2), ("micro_summary", 3)] {
        fields[key] = json!(row.get::<_, String>(column)?);
    }
    fields["children"] = serde_json::from_str(&row.get::<_, String>(4)?)?;
    let node: Node = serde_json::from_value(fields)?;
    require(
        crate::uuid(&node.id) && node.children.iter().all(|id| crate::uuid(id)),
        "malformed outline UUID metadata",
    )?;
    Ok(node)
}
/// Minimal discovery cards from the already-prepared outline projection.
/// Keep the same optional warnings and active status reasons as normal outlines.
pub(crate) fn selected_rows(connection: &Connection, ids: &[String]) -> Result<Vec<Value>> {
    let mut query = connection.prepare("SELECT fields FROM outline_rows WHERE id=?1")?;
    ids.iter().map(|id| {
        let fields: String = query.query_row([id], |row| row.get(0))?;
        let mut row: Value = serde_json::from_str(&fields)?;
        row.as_object_mut().ok_or("invalid outline row")?.remove("children");
        Ok(row)
    }).collect()
}

fn compact_nodes(connection: &Connection) -> Result<Vec<Node>> {
    let mut query = connection.prepare(&format!(
        "{COMPACT} WHERE id IN (SELECT id FROM outline_selected) ORDER BY id"
    ))?;
    let mut rows = query.query([])?;
    let mut nodes = Vec::new();
    while let Some(row) = rows.next()? {
        nodes.push(compact_node(row)?);
    }
    Ok(nodes)
}

// The draft projector never uses body text. Only the status-active reason and
// nonempty warnings can appear in rows; inactive metadata is not a dependency.
fn row_fingerprint(node: &Node) -> Result<String> {
    let mut row = serde_json::to_value(node)?;
    row.as_object_mut()
        .ok_or("missing compact node")?
        .remove("content");
    let status = row["status"]
        .as_str()
        .ok_or("missing compact status")?
        .to_owned();
    for (field, active) in [
        ("blocker", "stuck"),
        ("deferred_reason", "deferred"),
        ("discard_reason", "discarded"),
    ] {
        if status != active {
            row.as_object_mut().unwrap().remove(field);
        }
    }
    if row["warnings"].as_str() == Some("") {
        row["warnings"] = Value::Null;
    }
    Ok(sha256(&serde_json::to_vec(&row)?))
}

// Finite global structural checks are diagnostic only, not a replacement for
// publication's Bend law gate. UNION makes rootless/unreachable cycles terminate.
fn structural_diagnostic(connection: &Connection) -> Result<Option<String>> {
    Ok(connection.query_row("SELECT id||': '||detail FROM (
        SELECT source AS id,detail FROM relationship_mismatches
        UNION ALL SELECT c.source,'child link names missing node' FROM child_links c LEFT JOIN nodes n ON n.id=c.target WHERE n.id IS NULL
        UNION ALL SELECT target,'child has more than one parent' FROM child_links GROUP BY target HAVING count(*)>1
        UNION ALL SELECT r.id,'root shortcut names missing node' FROM tree_roots r LEFT JOIN nodes n ON n.id=r.id WHERE n.id IS NULL
        UNION ALL SELECT r.id,'root shortcut points to a child' FROM tree_roots r WHERE EXISTS(SELECT 1 FROM child_links c WHERE c.target=r.id)
        UNION ALL SELECT n.id,'parentless node has no root shortcut' FROM nodes n LEFT JOIN tree_roots r ON r.id=n.id WHERE r.id IS NULL AND NOT EXISTS(SELECT 1 FROM child_links c WHERE c.target=n.id)
        UNION ALL SELECT n.id,'cycle in supplied forest' FROM nodes n WHERE n.id NOT IN (
            WITH RECURSIVE reachable(id) AS (SELECT n.id FROM nodes n WHERE NOT EXISTS(SELECT 1 FROM child_links c WHERE c.target=n.id)
                UNION SELECT c.target FROM child_links c JOIN reachable r ON c.source=r.id) SELECT id FROM reachable)
        ) ORDER BY id,detail LIMIT 1", [], |r| r.get(0)).optional()?)
}

/// Refresh only affected trees. Old roots come from the previous projection and
/// new roots come from the current edges, so a move updates both sides.
pub(crate) fn refresh_indexed_from(
    connection: &Connection,
    root: &Path,
    changed: &BTreeSet<String>,
    force: bool,
) -> Result<u64> {
    connection.execute_batch(SCHEMA)?;
    let previous: Option<Option<String>> = connection
        .query_row(
            "SELECT diagnostic FROM outline_projection WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .optional()?;
    // Older updaters can publish once with their outgoing reader after the
    // incoming reader has already built this projection during conversion.
    if previous.is_none() || crate::index::residue::table_exists(connection, "outline_cache")? {
        crate::index::residue::retire_legacy_outlines(connection, root)?;
    }
    let all = force || previous != Some(None);
    connection.execute_batch("DELETE FROM outline_selected; DELETE FROM outline_changed;")?;
    let mut affected = BTreeSet::new();
    if !all {
        let mut source = connection.prepare(&format!("{COMPACT} WHERE id=?1"))?;
        let mut prior = connection.prepare("SELECT tree,input FROM outline_rows WHERE id=?1")?;
        for id in changed {
            let mut rows = source.query([id])?;
            let input = rows
                .next()?
                .map(compact_node)
                .transpose()?
                .as_ref()
                .map(row_fingerprint)
                .transpose()?;
            let old: Option<(String, String)> = prior
                .query_row([id], |row| Ok((row.get(0)?, row.get(1)?)))
                .optional()?;
            if input.as_ref() != old.as_ref().map(|(_, input)| input) {
                connection.execute("INSERT INTO outline_changed VALUES (?1)", [id])?;
                if let Some((tree, _)) = old {
                    affected.insert(tree);
                }
            }
        }
        let roots_changed: bool = connection.query_row(
            "SELECT EXISTS(SELECT id FROM tree_roots EXCEPT SELECT id FROM outline_rows WHERE parent IS NULL)
                OR EXISTS(SELECT id FROM outline_rows WHERE parent IS NULL EXCEPT SELECT id FROM tree_roots)
                OR EXISTS(SELECT 1 FROM relationship_mismatches)",
            [], |row| row.get(0),
        )?;
        let relevant: bool =
            connection.query_row("SELECT EXISTS(SELECT 1 FROM outline_changed)", [], |row| {
                row.get(0)
            })?;
        if !roots_changed && !relevant {
            return Ok(0);
        }
    }

    let diagnostic = structural_diagnostic(connection)?;
    let mut written = 0;
    if all || diagnostic.is_some() {
        connection.execute("DELETE FROM outline_rows", [])?;
    }
    if diagnostic.is_none() {
        let trees = if all {
            let mut query = connection.prepare("SELECT id FROM tree_roots ORDER BY id")?;
            query
                .query_map([], |row| row.get(0))?
                .collect::<rusqlite::Result<BTreeSet<String>>>()?
        } else {
            let mut query = connection.prepare(
                "WITH RECURSIVE ancestors(id) AS (
                    SELECT id FROM outline_changed
                    UNION SELECT c.source FROM child_links c JOIN ancestors a ON c.target=a.id
                ) SELECT id FROM tree_roots WHERE id IN (SELECT id FROM ancestors)",
            )?;
            affected.extend(
                query
                    .query_map([], |row| row.get(0))?
                    .collect::<rusqlite::Result<Vec<String>>>()?,
            );
            for tree in &affected {
                connection.execute("DELETE FROM outline_rows WHERE tree=?1", [tree])?;
            }
            affected
        };
        for tree in trees {
            connection.execute("DELETE FROM outline_selected", [])?;
            connection.execute(
                "WITH RECURSIVE branch(id) AS (
                    SELECT id FROM tree_roots WHERE id=?1
                    UNION ALL SELECT c.target FROM child_links c JOIN branch b ON c.source=b.id
                ) INSERT INTO outline_selected SELECT id FROM branch",
                [&tree],
            )?;
            written += write_tree(connection, &tree)?;
        }
    }
    connection.execute(
        "INSERT OR REPLACE INTO outline_projection VALUES (1,?1,?2)",
        params![processing(), diagnostic],
    )?;
    Ok(written)
}

fn write_tree(connection: &Connection, tree: &str) -> Result<u64> {
    let nodes = compact_nodes(connection)?;
    let forest = Forest::draft(&nodes)?;
    let mut rows = Vec::with_capacity(nodes.len());
    let mut prefix = Vec::with_capacity(nodes.len() + 1);
    prefix.push(0usize);
    let mut line = String::new();
    for (node, parent, row) in forest.indexed_rows() {
        let mut fields = serde_json::to_value(&row)?;
        let fields = fields.as_object_mut().ok_or("invalid outline row")?;
        fields.remove("descendants");
        fields.insert("depth".into(), json!(0));
        line.clear();
        write_full_outline_row(fields, &mut line)?;
        prefix.push(prefix.last().unwrap() + token_count::ordinary(&line));
        fields.remove("depth");
        rows.push((
            node,
            parent,
            row.depth,
            row.descendants,
            serde_json::to_string(fields)?,
        ));
    }
    let mut insert =
        connection.prepare("INSERT INTO outline_rows VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)")?;
    for (preorder, (node, parent, depth, descendants, fields)) in rows.into_iter().enumerate() {
        let tokens = prefix[preorder + descendants + 1] - prefix[preorder];
        insert.execute(params![
            node.id,
            tree,
            i64::try_from(preorder)?,
            parent,
            i64::try_from(depth)?,
            i64::try_from(descendants)?,
            fields,
            row_fingerprint(node)?,
            i64::try_from(tokens)?
        ])?;
    }
    Ok(nodes.len().try_into()?)
}

struct Branch {
    tree: String,
    start: i64,
    depth: i64,
    descendants: i64,
    row_tokens: i64,
}

impl Branch {
    fn read(row: &rusqlite::Row<'_>, column: usize) -> rusqlite::Result<Self> {
        Ok(Self {
            tree: row.get(column)?,
            start: row.get(column + 1)?,
            depth: row.get(column + 2)?,
            descendants: row.get(column + 3)?,
            row_tokens: row.get(column + 4)?,
        })
    }

    fn end(&self) -> i64 {
        self.start + self.descendants
    }

    fn tokens(&self, connection: &Connection, version: &str) -> Result<usize> {
        let header = full_outline_header(version, self.descendants + 1);
        let mut total = usize::try_from(self.row_tokens)? + token_count::ordinary(&header);
        if self.descendants < 1000 {
            return Ok(total);
        }
        // The pinned tokenizer encodes ASCII digits in groups of at most three,
        // each one token. Indexed row costs use depth zero. Only four or more
        // digits can add a token after rebasing to the requested owner.
        let mut query = connection.prepare_cached(
            "SELECT depth FROM outline_rows WHERE tree=?1 AND preorder BETWEEN ?2 AND ?3 AND depth>=1000 AND depth>=?4",
        )?;
        for depth in query.query_map(
            params![self.tree, self.start, self.end(), self.depth + 1000],
            |r| r.get::<_, i64>(0),
        )? {
            total += (depth? - self.depth).checked_ilog10().unwrap_or(0) as usize / 3;
        }
        Ok(total)
    }
}

pub(crate) fn indexed_outline(
    connection: &Connection,
    prefix: &str,
    coverage: Coverage,
    version: &str,
    limit: Option<usize>,
) -> Result<super::Outline> {
    require(
        !prefix.is_empty()
            && prefix.len() <= 36
            && prefix.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "invalid UUID prefix",
    )?;
    let mut ids =
        connection.prepare("SELECT id FROM nodes WHERE id>=?1 AND id<?2 ORDER BY id LIMIT 2")?;
    let ids = ids
        .query_map(params![prefix, format!("{prefix}g")], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    require(
        ids.len() == 1 && ids[0].starts_with(prefix),
        "UUID prefix absent or ambiguous",
    )?;
    let id = &ids[0];
    let label = match coverage {
        Coverage::Shallow => "shallow",
        Coverage::Full => "full",
        Coverage::Asc => "asc",
    };
    let diagnostic: Option<String> = connection.query_row(
        "SELECT diagnostic FROM outline_projection WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    if let Some(diagnostic) = diagnostic {
        let document = json!({"coverage":"invalid_draft","requested_coverage":label,
            "status":"invalid_draft","owner":id,"structural_diagnostic":diagnostic,
            "version":version,"path_template":PATH_TEMPLATE,"rows":[]});
        return Ok(super::Outline::Text(lean_render::render(&document)? + "\n"));
    }
    if coverage == Coverage::Asc {
        let mut query = connection.prepare(
            "WITH RECURSIVE ancestors(id,parent,fields,hops) AS (
                SELECT id,parent,fields,0 FROM outline_rows WHERE id=?1
                UNION ALL
                SELECT r.id,r.parent,r.fields,a.hops+1
                FROM outline_rows r JOIN ancestors a ON r.id=a.parent
            ) SELECT fields,hops FROM ancestors ORDER BY hops",
        )?;
        let mut rows = query.query([id])?;
        let mut projected = Vec::new();
        while let Some(row) = rows.next()? {
            let mut fields: Value = serde_json::from_str(row.get_ref(0)?.as_str()?)?;
            fields["depth"] = json!(row.get::<_, i64>(1)?);
            projected.push(fields);
        }
        require(!projected.is_empty(), "missing outline row; run sifttext index STORE --repair")?;
        let document = json!({"coverage":label,"version":version,"path_template":PATH_TEMPLATE,"rows":projected});
        return Ok(super::Outline::Text(lean_render::render(&document)? + "\n"));
    }
    if coverage == Coverage::Full {
        let branch = connection.query_row(
            "SELECT tree,preorder,depth,descendants,subtree_tokens FROM outline_rows WHERE id=?1",
            [id],
            |row| Branch::read(row, 0),
        )?;
        if let Some(limit) = limit {
            let tokens = branch.tokens(connection, version)?;
            if tokens > limit {
                return Ok(super::Outline::Suppressed(tokens));
            }
        }
        let mut text = full_outline_header(version, branch.descendants + 1);
        let mut query = connection.prepare(
            "SELECT depth,fields FROM outline_rows WHERE tree=?1 AND preorder BETWEEN ?2 AND ?3 ORDER BY preorder",
        )?;
        let mut rows = query.query(params![branch.tree, branch.start, branch.end()])?;
        while let Some(row) = rows.next()? {
            let depth: i64 = row.get(0)?;
            let mut fields: serde_json::Map<String, Value> =
                serde_json::from_str(row.get_ref(1)?.as_str()?)?;
            fields.insert("depth".into(), json!(depth - branch.depth));
            write_full_outline_row(&fields, &mut text)?;
        }
        return Ok(super::Outline::Text(text));
    }
    let mut query = connection.prepare(
        "SELECT fields,tree,preorder,depth,descendants,subtree_tokens,0 AS relative_depth FROM outline_rows WHERE id=?1
         UNION ALL
         SELECT fields,tree,preorder,depth,descendants,subtree_tokens,1 FROM outline_rows WHERE parent=?1
         ORDER BY relative_depth,preorder",
    )?;
    let mut rows = query.query([id])?;
    let mut projected = Vec::new();
    while let Some(row) = rows.next()? {
        let mut fields: Value = serde_json::from_str(row.get_ref(0)?.as_str()?)?;
        fields["depth"] = json!(row.get::<_, i64>(6)?);
        let branch = Branch::read(row, 1)?;
        if branch.descendants > 0 {
            fields["full_view_tokens"] = json!(branch.tokens(connection, version)?);
        }
        projected.push(fields);
    }
    require(
        !projected.is_empty(),
        "missing outline row; run sifttext index STORE --repair",
    )?;
    let document = json!({"coverage":label,"version":version,"path_template":PATH_TEMPLATE,"rows":projected});
    Ok(super::Outline::Text(lean_render::render(&document)? + "\n"))
}
