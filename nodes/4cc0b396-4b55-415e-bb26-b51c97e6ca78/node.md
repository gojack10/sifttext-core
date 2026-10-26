---
id: "4cc0b396-4b55-415e-bb26-b51c97e6ca78"
name: "SQLite Queries"
micro_summary: "Explains read-only discovery and filtering of the local derived index, including freshness checks and its distinction from hosted PostgreSQL."
status: "in_progress"
see_also: []
scope: "Use the actual local SQLite schema safely, including parent/root filtering, and return verifiable source identities."
warnings: ""
---
Use for precise field filters, counts or relationships. Input: a confirmed `store` path and the bounded question. The wrapper refreshes and queries current parseable files in the selected working copy, including uncommitted incomplete drafts, in a same-version transaction. Cycles or missing relationships do not impose a whole-store Bend acceptance gate on SQL; rows mirror authored data, not certified publication. Malformed or unsafe source still refuses, never silently yielding stale or partial success. Refresh maintains SQLite and eager native outlines together; an outline can explicitly report `invalid_draft` where honest navigation is impossible. Use one `SELECT` statement; it rejects mutation statements and more than 1000 result rows. Use `count(*)` to count rather than fetching rows to count them. Select only needed fields, with explicit filters and a small `LIMIT`, not `SELECT *`. Do not modify the database directly.

## Local schema

- `nodes(id, name, micro_summary, scope, content, status, children, record)` — text columns; `children` is an ordered JSON UUID array and `record` is the complete derived JSON row.
- `links(source, target, reason, position)` — explicit authored see-also edges, not parent/child relationships.
- `metadata(key, value)` — derived cache metadata. `version` is an opaque 64-hex `authored-merkle-v2` inventory identity, not a Git commit or JSON-file hash; old cache/version formats invalidate once. `outlines` is internal metadata, not a retrieval result or version-comparison target.

There is no hosted `tree_id`, `parent_id`, priority or separate `trees` table here. Derive parentage from `children`; roots have no parent. The KB root is `38a9fc79-8a84-494d-a962-2ec5134bc856`.

```sh
sifttext sql "$store" 'SELECT id,name,status FROM nodes ORDER BY name LIMIT 20'
sifttext sql "$store" "SELECT id,name FROM nodes WHERE name LIKE '%SQLite%' LIMIT 20"
sifttext sql "$store" 'SELECT p.id AS parent_id,c.value AS child_id FROM nodes p,json_each(p.children) c LIMIT 20'
sifttext sql "$store" 'SELECT n.id,n.name FROM nodes n WHERE NOT EXISTS (SELECT 1 FROM nodes p,json_each(p.children) c WHERE c.value=n.id) LIMIT 20'
sifttext sql "$store" "SELECT n.id,n.name FROM nodes p,json_each(p.children) c JOIN nodes n ON n.id=c.value WHERE p.id='5e565245-14fe-406c-977c-cf94a9be246b' ORDER BY CAST(c.key AS INTEGER)"
```

For recursive subtree navigation use Outline Navigation or scoped Leviathan Search rather than inventing a recursive-query command the wrapper does not accept. Query results are LEAN: `status`, authored-store `version`, zero-based `column_0`, `column_1`, … name metadata, and scalar-valued `rows`. Each row uses positional `c0`, `c1`, … keys, not SQL names, preserving duplicate, empty and punctuation-bearing column names even when no rows match. Text remains text and SQL NULL is LEAN `_`. Numeric values are decimal strings. When all non-NULL values in a column share one numeric storage class, a top-level `c0_type:integer` or `c0_type:real` applies to that column (and likewise for other positions). Mixed columns retain the numeric type on each numeric cell's row. NULL stays NULL regardless of a column type. Parse numbers using that storage class rather than treating numeric-looking text as a number. This preserves 64-bit integers, real values including negative zero and infinities, and mixed storage classes across rows through LEAN decoders limited to JavaScript numbers. Without a column or cell type tag, a value is text or NULL. Blob results remain unsupported. The version is comparable with search and outline for the same authored store. Read the relevant `nodes/UUID/node.md` before interpreting results as guidance or deciding edits.

Each working copy owns its `.sifttext/index.sqlite`; do not merge databases. A manually opened cache or raw generated outline may be stale between authored edits and refresh; it is not a supported write API. Unchanged queries use read transactions; per-view refresh coordination reuses unchanged hashes/records and updates dependent outlines. Interrupted dirty generations/boot epochs require native repair, not manual journal deletion. These mechanisms do not promise every SQL query is instant or below 50 MB. On a noncurrent view, malformed source or query rejection, report the error and fix the query or source within authority before retrying. Return the exact query and relevant rows/version with the answer; do not infer completeness from a truncated result.
