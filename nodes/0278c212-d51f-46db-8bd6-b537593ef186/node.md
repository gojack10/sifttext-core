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

The optional last argument scopes to that node and its descendants. Without it, search spans the local forest. This is ranked lexical Leviathan retrieval over the native index, not semantic embeddings or the hosted MCP API. It is local to the selected working copy, including its valid uncommitted authored changes. The convenience CLI returns at most 20 hits plus `total_matches`, `status`, `version` and `version_scope:authored_store`. This version covers the selected authored store, not an outline's displayed-view fingerprint. Refine the query or subtree if matches exceed the displayed page; do not claim all results were examined.

Use returned UUIDs to read actual `nodes/UUID/node.md`; use Outline Navigation for surrounding structure. A match is a candidate source, not a proven answer or authorization. No hits means this lexical query did not find evidence, not that the subject is absent. Try a more precise/different phrase or a structural SQL query when that distinction matters.

Rejection or a noncurrent result stops dependent reasoning. Inspect the authored store and rerun the complete check before retrying; never silently substitute stale results. Return the supported answer with source UUIDs/version and any remaining uncertainty to the original task.
