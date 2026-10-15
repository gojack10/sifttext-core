// Thin native observation surface for the vendored Leviathan relevance ordering and paging.
//
// One JSON request `{hits, limit, offset}` on stdin, one JSON document on stdout plus a final
// newline. The result is exactly `{overfetch, results}`: the page the pinned upstream
// `Store::ranked` relevance branch would produce from the already-selected hits in `hits`, with
// `results` entries `{rowid, rank, relevance}`.
//
// `hits` are the rows a caller has already selected and ordered in SQL. Each one carries exactly
// the upstream columns `rowid`, `bm25`, `boost` and `date`; `date` is required and is `null` or a
// string, and an absent date becomes the empty string exactly as upstream `unwrap_or_default()`
// does. `limit` and `offset` are nonnegative integers: the request limit is clamped to 1..=50 by
// the vendored code, so a zero limit still yields one row and anything above 50 yields at most 50.
//
// A rejected request writes nothing to stdout and exits nonzero with the reason on stderr. That
// covers malformed or truncated JSON, a non-object document, a missing or unknown field at either
// level, a non-integer or negative `limit`/`offset` or one outside `u32`, a non-integer `rowid`, an
// absent `date` key or one that is neither `null` nor a string, a non-finite `bm25`/`boost`, a
// product `bm25 * boost` that is not finite, and a display numerator `-score * 100.0` that is not
// finite. These are transport bounds on this surface, not a new upstream search policy: upstream
// reads those values out of SQLite and does not validate them.
//
// This is a test/inspection surface only: no SQLite, no FTS5 execution, no candidate retrieval, no
// BM25 computation, no query parsing, no group resolution, scope, filter, date-bound, browse or
// newest behavior, no other-groups fallback, no record/`Mapping` extraction, no card or public
// result construction, no filesystem read or write, no persistence and no source mutation. An empty
// `hits` vector is a legitimate probe of this pure surface; it is not a valid authored store and
// nothing here reads or generates one.

#[path = "../vendor/leviathan/relevance.rs"]
mod relevance;

use serde::Deserialize;
use serde_json::Value;
use std::io::{Read, Write};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    hits: Vec<Supplied>,
    limit: u32,
    offset: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Supplied {
    rowid: i64,
    bm25: f64,
    boost: f64,
    // `Value`, not `Option<String>`: serde treats an absent `Option` field as `None`, while this
    // transport requires the key and accepts only `null` or a string for it.
    date: Value,
}

fn main() {
    let mut supplied = String::new();
    // ponytail: whole-request read, no size cap. This observer is a local test surface driven by
    // the external gate; cap the read if it ever becomes an untrusted input path.
    if let Err(error) = std::io::stdin().read_to_string(&mut supplied) {
        eprintln!("rejected: unreadable request: {error}");
        std::process::exit(1);
    }
    match page_json(&supplied) {
        Ok(text) => {
            let handle = std::io::stdout();
            let mut stdout = handle.lock();
            if let Err(error) = writeln!(stdout, "{text}").and_then(|()| stdout.flush()) {
                eprintln!("rejected: unwritable result: {error}");
                std::process::exit(1);
            }
        }
        Err(reason) => {
            eprintln!("rejected: {reason}");
            std::process::exit(1);
        }
    }
}

/// The whole accepted boundary: parse one request, keep only hits whose upstream arithmetic is
/// finite inside this transport, and serialize exactly `{overfetch, results}`.
/// Public to this crate's test module only.
fn page_json(supplied: &str) -> Result<String, String> {
    let request: Request =
        serde_json::from_str(supplied).map_err(|error| format!("malformed relevance request: {error}"))?;
    let mut hits = Vec::with_capacity(request.hits.len());
    for hit in request.hits {
        let date = match hit.date {
            Value::Null => None,
            Value::String(text) => Some(text),
            other => return Err(format!("rowid {} has a non-string date: {other}", hit.rowid)),
        };
        if !hit.bm25.is_finite() || !hit.boost.is_finite() {
            return Err(format!("rowid {} has a non-finite bm25 or boost", hit.rowid));
        }
        let score = hit.bm25 * hit.boost;
        if !score.is_finite() {
            return Err(format!("rowid {} has a non-finite score {}", hit.rowid, score));
        }
        if !(-score * 100.0).is_finite() {
            return Err(format!("rowid {} has a non-finite display score {}", hit.rowid, score));
        }
        hits.push(relevance::Hit { rowid: hit.rowid, bm25: hit.bm25, boost: hit.boost, date });
    }
    // Widened on this native target: upstream arithmetic is `usize`, and a `u32` request bound
    // would overflow before the clamp if it were kept at 32 bits here.
    let page = relevance::page(&hits, request.limit as usize, request.offset as usize);
    serde_json::to_string(&page).map_err(|error| error.to_string())
}

// Registered through this crate until Cargo/lib test registration lands. This binary owns the one
// `#[path]` declaration of the vendored ordering, so the test module uses it without loading that
// source twice.
#[cfg(test)]
#[path = "../../tests/successor/leviathan_relevance.rs"]
mod leviathan_relevance_paging;
