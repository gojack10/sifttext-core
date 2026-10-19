---
id: "0278c212-d51f-46db-8bd6-b537593ef186"
name: "Leviathan Search"
micro_summary: "Guides agents from forest or subtree lexical matches to identified source nodes, with scope-aware follow-up reads."
status: "in_progress"
see_also: []
scope: "Search the local forest or a known subtree without assuming hosted or semantic search behavior."
warnings: ""
---
Use when a task gives a phrase or subject rather than a known node. Input: `store`, the confirmed home/worktree, and distinguishing search text.

```sh
sifttext search "$store" 'private Git worktree'
sifttext search "$store" 'SQLite' 5e565245-14fe-406c-977c-cf94a9be246b
```

The optional last argument scopes to that node and its descendants. Without it, search spans the local forest. This is ranked lexical Leviathan retrieval over the native index, not semantic embeddings or the hosted MCP API. It is local to the selected working copy, including its valid uncommitted authored changes. The CLI returns LEAN with at most 20 ranked hits plus `total_matches`, `status`, `version` and `version_scope:authored_store`. Its `rows` merge each hit with its complete root-to-hit ancestor chain from the same checked source. `hit` is the one-based ranked hit number, `depth` starts at zero for the root, and `match:T` marks the actual hit; ancestor context does not add matches. Hits retain their excerpt, canonical path, status, active reason and warnings when present. Ancestors carry ID, name, micro-summary, status and applicable reason/warnings. Filtered callers receive `unavailable:T` rows at unreadable ancestor positions without their IDs or text; do not infer direct adjacency across those gaps. This version covers the selected authored store, not an outline's displayed-view fingerprint. Refine the query or subtree if matches exceed the displayed page; do not claim all results were examined.

Read relevant `match:T` rows as candidate sources and use returned UUIDs to open actual `nodes/UUID/node.md`; use ancestor rows for orientation and Outline Navigation for other surrounding structure. A match is a candidate source, not a proven answer or authorization. No hits means this lexical query did not find evidence, not that the subject is absent. Try a more precise/different phrase or a structural SQL query when that distinction matters.

Rejection or a noncurrent result stops dependent reasoning. Inspect the authored store and rerun the complete check before retrying; never silently substitute stale results. Return the supported answer with source UUIDs/version and any remaining uncertainty to the original task.
