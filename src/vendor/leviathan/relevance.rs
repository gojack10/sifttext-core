// Vendored upstream relevance ordering and paging, extracted from Leviathan.
//
// Pinned source: revision 702ea92dc4695de00c488a66b43d75419bbb78e8, file `src/query.rs`
// (SHA-256 ec7e595c8ea18adec60986d870cc0ed5a6b86bebeaef02c4d21d60b5a9277273).
// License Apache-2.0; the unmodified upstream `LICENSE` and `NOTICE` sit beside this file and
// `THIRD_PARTY.md` records the vendored query region. This file is an extracted and modified
// upstream region, not a whole-file copy.
//
// Copied verbatim as upstream expressions, with the pinned line numbers of that revision:
// `MAX_LIMIT` (`src/query.rs:30`), the request clamp `limit.clamp(1, MAX_LIMIT)` from
// `Store::search` (`:310`), `want = offset + limit` and `(want * 10).max(100)` from
// `Store::ranked` (`:456`-`:457`, the `Sort::Relevance` branch), the row projection
// `score: bm25 * boost` with `date: ...unwrap_or_default()` (`:484`-`:488`), the ordering
// `sort_by(|a, b| a.score.total_cmp(&b.score).then_with(|| b.date.cmp(&a.date)))` (`:493`) and the
// page `skip(offset).take(limit).enumerate()` with `rank = offset + i + 1` and
// `relevance = (-score * 100.0).round() / 100.0` (`:496`-`:503`).
//
// Modified, and this is the whole adaptation: upstream `Store::ranked` selects its own hits by
// running an FTS5 query and reading `(rowid, boost, date, rank)` rows out of rusqlite. Here the
// already-selected, SQL-ordered hits arrive as an argument, so text ranking stays separate from
// candidate retrieval, SQLite and record mapping. Nothing else is changed: no status boosts, no
// different cutoff, no re-scaled score, no different tie rule, no rounding-mode swap.
//
// Deliberately not ported, because they are not part of this ordering: the SQL construction and
// its parameters, the `count(*) OVER ()` total, `Sort::Newest` and `Store::browse`, the group
// resolution, scope, filter, since/until and other-groups fallback behavior, `Store::card` and
// every `Card` field, and the `Plan`/`group_row` helpers. A later integrated search owns those,
// together with the ancestry, access and same-working-version adaptation the product contract
// requires. `overfetch` is returned because it is part of the upstream request to SQL; it does not
// truncate the hits supplied here, which are already the selected set.
//
// Exact score/date ties keep the order of the supplied vector. That is Rust's stable sort acting on
// the argument order, not an upstream guarantee about SQLite tie order, and it must not be claimed
// as one.
//
// Nothing here parses a query, retrieves a candidate, opens a database, reads a file, maps a record
// or builds a card.

use serde::Serialize;

/// Upstream `MAX_LIMIT`: the ceiling `Store::search` clamps every request limit to.
pub const MAX_LIMIT: usize = 50;

/// One already-selected hit: exactly the three columns upstream `Store::ranked` reads per row.
#[derive(Debug, Clone)]
pub struct Hit {
    pub rowid: i64,
    pub bm25: f64,
    pub boost: f64,
    pub date: Option<String>,
}

/// One displayed result: upstream `Card::rank` and `Card::relevance` for one hit of the page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub rowid: i64,
    pub rank: usize,
    pub relevance: f64,
}

/// The upstream relevance page plus the overfetch the caller would have asked SQL for.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Page {
    pub overfetch: usize,
    pub results: Vec<Row>,
}

/// The relevance-only part of upstream `Store::ranked: score each already-selected hit by
/// `bm25 * boost`, order by ascending score then descending date keeping supplied order for exact
/// ties, then page and number the result.
pub fn page(hits: &[Hit], limit: usize, offset: usize) -> Page {
    let limit = limit.clamp(1, MAX_LIMIT);
    let want = offset + limit;
    let overfetch = (want * 10).max(100);
    let mut ordered: Vec<(f64, String, i64)> = hits
        .iter()
        .map(|hit| (hit.bm25 * hit.boost, hit.date.clone().unwrap_or_default(), hit.rowid))
        .collect();
    ordered.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
    let results = ordered
        .into_iter()
        .skip(offset)
        .take(limit)
        .enumerate()
        .map(|(index, (score, _, rowid))| Row {
            rowid,
            rank: offset + index + 1,
            relevance: (-score * 100.0).round() / 100.0,
        })
        .collect();
    Page { overfetch, results }
}
