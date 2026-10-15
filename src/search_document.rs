// Pure SiftText-to-searchable-document projection.
//
// One authored node's four covered fields in, the two text fields a searchable document carries
// out: `title` is the authored `name` and `body` is the authored `micro_summary`, `scope` and
// `crystallization` joined by LF in that order. That is the whole mapping. It is the same projection
// the maintained acceptance contract and the external search expectation use, so a candidate text
// document and a real indexed node agree by construction rather than by a second spelling.
//
// Nothing here loads a graph, parses Markdown, opens SQLite, hydrates a store, refreshes an index,
// filters by scope or access, ranks, pages, snippets, computes tokens, writes a file or performs any
// other side effect. It takes four text values and returns two.
//
// Authored text is never rewritten. There is no trimming, flattening, line-ending normalization,
// collapsing, truncation, reordering or omission: every character, including leading and trailing
// whitespace, embedded LF, CR, NUL, Unicode, and literal reference, metadata or code text, survives
// byte-for-byte. `name` is not repeated inside `body`, and no generated context, status, reason,
// relationship, boost or token data is invented or copied in.
//
// Only two validity rules belong to this projection, because the searchable surface cannot carry a
// node whose display title or summary text is blank: authored `name` and `micro_summary` must be
// nonblank. Blank means empty or entirely Unicode whitespace, and it is rejected rather than
// repaired. `scope` and `crystallization` may legitimately be empty strings, so an empty one of
// those still contributes its separator. This helper does not decide status-specific validity of
// `crystallization`, does not accept a complete authored store, and does not certify that the record
// it was handed is a valid node: a caller that has already validated the store normalizes an absent
// optional authored `scope` to the empty string before asking for a projection, and that
// normalization is not this helper's business.
//
// Not yet registered in src/lib.rs: `src/bin/search-document-observe.rs` includes this module by
// path so the pure contribution stays inside its own files until library registration lands. Later
// real search must consume this same `project`, not a re-derived copy of the separator or order.

use serde::Serialize;

/// The four authored fields one searchable document covers, in coverage order.
#[derive(Debug, Clone, Copy)]
pub struct Fields<'a> {
    pub name: &'a str,
    pub micro_summary: &'a str,
    pub scope: &'a str,
    pub crystallization: &'a str,
}

/// One searchable document: exactly the `title` and `body` text an index consumes per node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Document<'a> {
    pub title: &'a str,
    pub body: String,
}

/// `name` becomes `title`; `micro_summary`, `scope` and `crystallization` become `body`, joined by
/// LF in that order. Errs only on a blank authored `name` or `micro_summary`.
pub fn project<'a>(fields: Fields<'a>) -> Result<Document<'a>, &'static str> {
    if is_blank(fields.name) {
        return Err("blank authored name");
    }
    if is_blank(fields.micro_summary) {
        return Err("blank authored micro-summary");
    }
    Ok(Document {
        title: fields.name,
        body: [fields.micro_summary, fields.scope, fields.crystallization].join("\n"),
    })
}

/// Empty, or every character Unicode `White_Space`. Used only as a rejection test: the projection
/// itself never receives a trimmed value.
fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}
