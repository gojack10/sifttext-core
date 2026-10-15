// Adapted Leviathan src/index.rs:63-73 and src/query.rs:438-566 at
// 702ea92dc4695de00c488a66b43d75419bbb78e8 (Apache-2.0).
// Reuses accepted query, relevance and card regions. Differences: a caller's
// checked SQLite transaction/UUID rows replace Store/file stamps; one bounded
// permitted group replaces fuzzy groups; no fallback, dates or custom filters.
// Synthetic names are inert hashed group tokens, preserving upstream column
// lengths without removing real authored text or making group labels searchable.
// See THIRD_PARTY.md in this directory for the adaptation boundary.
use crate::{Result, card, leviathan_query::Query, relevance};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;
use std::collections::BTreeMap;

pub struct Document<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub permitted: bool,
    pub boost: f64,
}

#[derive(Debug, Serialize)]
pub struct Card {
    #[serde(flatten)]
    pub text: card::Card,
    pub group: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance: Option<f64>,
}

// Upstream text::fnv1a/group_token, one group part (no separators needed).
fn group_token(key: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("g{h:016x}")
}

/// Maintain only verified text. No query, scope, permission, or boost persists.
pub fn maintain(tx: &Transaction<'_>, documents: &[Document<'_>]) -> Result<BTreeMap<i64, usize>> {
    // Content-backed FTS lets SQLite verify postings against their stored text;
    // compare that text to the complete checked source, never trust a version stamp.
    const SCHEMA: &str = "CREATE VIRTUAL TABLE search_fts USING fts5(title,body,names,tags, tokenize='porter unicode61')";
    let schema: Option<String> = tx
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='search_fts'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if schema.as_deref() != Some(SCHEMA) {
        tx.execute_batch("DROP TABLE IF EXISTS search_fts;")?;
        tx.execute_batch(SCHEMA)?;
    }
    // This FTS5 command checks content/posting checksums without changing pages.
    // Corruption is an explicit error; no damaged result is labeled current.
    tx.execute(
        "INSERT INTO search_fts(search_fts) VALUES ('integrity-check')",
        [],
    )?;
    let existing: BTreeMap<i64, Vec<String>> = tx
        .prepare("SELECT rowid,title,body,names,tags FROM search_fts ORDER BY rowid")?
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                (1..5).map(|i| r.get(i)).collect::<rusqlite::Result<_>>()?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    // Stable identities prevent a middle UUID insertion from rewriting unrelated
    // postings. Ordering for browse remains the caller's upstream logical order.
    tx.execute_batch("CREATE TABLE IF NOT EXISTS search_ids (rowid INTEGER PRIMARY KEY, id TEXT NOT NULL UNIQUE)")?;
    let ids: BTreeMap<String, i64> = tx
        .prepare("SELECT id,rowid FROM search_ids")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut next = existing
        .keys()
        .copied()
        .chain(ids.values().copied())
        .max()
        .unwrap_or(0)
        .max(0);
    let mut by_rowid = BTreeMap::new();
    for (i, doc) in documents.iter().enumerate() {
        let rowid = match ids.get(doc.id).filter(|n| **n > 0) {
            Some(n) => *n,
            None => {
                next = next.checked_add(1).ok_or("search rowid overflow")?;
                next
            }
        };
        crate::require(
            by_rowid.insert(rowid, i).is_none(),
            "duplicate searchable identity",
        )?;
    }
    let token = group_token("permitted");
    let (mut removed, mut updated) = (0usize, 0usize);
    for rowid in existing
        .keys()
        .filter(|rowid| !by_rowid.contains_key(rowid))
    {
        tx.execute("DELETE FROM search_fts WHERE rowid=?1", [rowid])?;
        removed += 1;
    }
    for (id, rowid) in &ids {
        if by_rowid.get(rowid).is_none_or(|i| documents[*i].id != id) {
            tx.execute("DELETE FROM search_ids WHERE id=?1", [id])?;
        }
    }
    for (&rowid, &i) in &by_rowid {
        let doc = &documents[i];
        tx.execute("INSERT INTO search_ids(rowid,id) VALUES (?1,?2) ON CONFLICT(rowid) DO UPDATE SET id=excluded.id WHERE id<>excluded.id", params![rowid,doc.id])?;
        let wanted = [doc.title.trim(), doc.body, &token, &token];
        if !existing
            .get(&rowid)
            .is_some_and(|values| values.iter().map(String::as_str).eq(wanted))
        {
            updated += 1;
            tx.execute("DELETE FROM search_fts WHERE rowid=?1", [rowid])?;
            tx.execute(
                "INSERT INTO search_fts(rowid,title,body,names,tags) VALUES (?1,?2,?3,?4,?4)",
                params![rowid, doc.title.trim(), doc.body, token],
            )?;
        }
    }
    if std::env::var_os("SIFTTEXT_RETRIEVAL_TRACE").is_some() {
        eprintln!(
            "retrieval-postings {}",
            serde_json::json!({"updated":updated,"removed":removed,"verified":documents.len()})
        );
    }
    Ok(by_rowid)
}

pub fn retrieve(
    tx: &Transaction<'_>,
    documents: &[Document<'_>],
    query: &str,
    limit: usize,
    offset: usize,
    title_weight: f64,
) -> Result<(i64, Vec<Card>)> {
    let by_rowid = maintain(tx, documents)?;
    // Invocation-only permissions and weights never enter persisted postings.
    // All synthetic group columns keep their upstream lengths and zero tag weight.
    let permitted: String = serde_json::to_string(
        &by_rowid
            .iter()
            .filter(|(_, i)| documents[**i].permitted)
            .map(|(rowid, _)| *rowid)
            .collect::<Vec<_>>(),
    )?;
    let parsed = Query::parse(query);
    let limit = limit.clamp(1, relevance::MAX_LIMIT);
    let mut total = 0;
    let page = if parsed.is_empty() {
        // Upstream browse: date DESC,rowid DESC; mapping here has no dates.
        let rowids_by_position: BTreeMap<_, _> =
            by_rowid.iter().map(|(rowid, i)| (*i, *rowid)).collect();
        let rowids: Vec<_> = rowids_by_position
            .iter()
            .rev()
            .filter(|(i, _)| documents[**i].permitted)
            .map(|(_, rowid)| *rowid)
            .collect();
        total = rowids.len() as i64;
        rowids
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|rowid| (rowid, None, None))
            .collect::<Vec<_>>()
    } else {
        let positive = parsed.fts().unwrap();
        let mut expr = format!(
            "tags : \"{}\" AND {{title body}} : ({positive})",
            group_token("permitted")
        );
        if let Some(negative) = parsed.not_fts() {
            expr = format!("({expr}) NOT ({negative})");
        }
        let overfetch = ((offset + limit) * 10).max(100) as i64;
        // Exact upstream candidate cutoff/count window, before boosts and paging.
        let hits = tx
            .prepare(
                "WITH hits AS MATERIALIZED (
                    SELECT rowid,bm25(search_fts,?4,1.0,0.5,0.0) AS score FROM search_fts
                    WHERE search_fts MATCH ?1 AND rowid IN (SELECT value FROM json_each(?3)))
                 SELECT rowid,score,count(*) OVER () FROM hits ORDER BY score LIMIT ?2",
            )?
            .query_map(params![expr, overfetch, permitted, title_weight], |row| {
                total = row.get(2)?;
                let rowid: i64 = row.get(0)?;
                Ok(relevance::Hit {
                    rowid,
                    bm25: row.get(1)?,
                    boost: documents[by_rowid[&rowid]].boost,
                    date: None,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        relevance::page(&hits, limit, offset)
            .results
            .into_iter()
            .map(|r| (r.rowid, Some(r.rank), Some(r.relevance)))
            .collect()
    };
    let highlight = parsed.highlight();
    let cards = page
        .into_iter()
        .map(|(rowid, rank, relevance)| {
            let doc = &documents[by_rowid[&rowid]];
            let mut text = card::build(doc.id, doc.title, doc.body, &highlight, 300);
            // The integrated upstream mapping is title=[title], text=[body],
            // unlike the pure card helper's text=[title,body]. A clipped long
            // title must not enter the snippet pool merely because it is indexed.
            let shown = text.title.iter().cloned().collect::<Vec<_>>();
            text.snippet = crate::snippet::select(&[doc.body.to_owned()], &highlight, &shown, 300);
            Card {
                text,
                group: "permitted",
                rank,
                relevance,
            }
        })
        .collect();
    Ok((total, cards))
}
