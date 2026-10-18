---
id: "4cc0b396-4b55-415e-bb26-b51c97e6ca78"
name: "SQLite Queries"
micro_summary: "Explains read-only discovery and filtering of the local derived index, including freshness checks and its distinction from hosted PostgreSQL."
status: "in_progress"
see_also: []
scope: "Use the actual local SQLite schema safely, including parent/root filtering, and return verifiable source identities."
warnings: ""
---
Use for precise field filters, counts or relationships. Input: a confirmed `store` path and the bounded question. The wrapper refreshes and queries the selected working copy inside the existing native same-version transaction. Use one `SELECT` statement; it rejects mutation statements and more than 1000 result rows. Prefer a small explicit `LIMIT`, not `SELECT *`. Do not modify the database directly.

## Local schema

- `nodes(id, name, micro_summary, scope, content, status, children, record)` — text columns; `children` is an ordered JSON UUID array and `record` is the complete derived JSON row.
- `links(source, target, reason, position)` — explicit authored see-also edges, not parent/child relationships.
- `metadata(key, value)` — derived cache metadata. `version` fingerprints the authored store; `outlines` is an internal generated-file manifest, not a retrieval result or a version-comparison target.

There is no hosted `tree_id`, `parent_id`, priority or separate `trees` table here. Derive parentage from `children`; roots have no parent. The KB root is `38a9fc79-8a84-494d-a962-2ec5134bc856`.

```sh
sifttext sql "$store" 'SELECT id,name,status FROM nodes ORDER BY name LIMIT 20'
sifttext sql "$store" "SELECT id,name FROM nodes WHERE name LIKE '%SQLite%' LIMIT 20"
sifttext sql "$store" 'SELECT p.id AS parent_id,c.value AS child_id FROM nodes p,json_each(p.children) c LIMIT 20'
sifttext sql "$store" 'SELECT n.id,n.name FROM nodes n WHERE NOT EXISTS (SELECT 1 FROM nodes p,json_each(p.children) c WHERE c.value=n.id) LIMIT 20'
sifttext sql "$store" "SELECT n.id,n.name FROM nodes p,json_each(p.children) c JOIN nodes n ON n.id=c.value WHERE p.id='5e565245-14fe-406c-977c-cf94a9be246b' ORDER BY CAST(c.key AS INTEGER)"
```

For recursive subtree navigation use Outline Navigation or scoped Leviathan Search rather than inventing a recursive-query command the wrapper does not accept. Query results report `status`, `version`, `version_scope:authored_store`, columns and row arrays. The version covers the selected authored store, not an outline's displayed-view fingerprint. Read the relevant `nodes/UUID/node.md` before interpreting results as guidance or deciding edits.

A manually opened `.sifttext/index.sqlite` is only a cache and may be stale; it is not a supported write API. On a noncurrent view, malformed source or query rejection, report the error and fix the query or source within authority before retrying. Return the exact query and relevant rows/version with the answer; do not infer completeness from a truncated result.
