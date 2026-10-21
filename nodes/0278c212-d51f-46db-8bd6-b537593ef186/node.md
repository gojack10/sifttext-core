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

The optional last argument scopes to that node and its descendants. Without it, search spans the local forest. This is ranked lexical Leviathan retrieval over the native index, not semantic embeddings or the hosted MCP API. It is local to the selected working copy, including its valid uncommitted authored changes. The CLI returns LEAN with at most 20 ranked actual hits in `rows`, plus `total_matches`, `status`, authored-store `version` and one `path_template`. Names and micro-summaries are preserved intact. Each hit keeps its ID and status, with root/parent IDs for compact context when it has ancestors, and a matching excerpt only when it adds information beyond its displayed name/summary. There are no expanded ancestor rows or per-hit absolute paths. Applicable warnings along readable ancestor chains appear once per source as `warnings_UUID`. Warnings over 512 tokens instead return `must_read_warnings_UUID` with the source path: read that source's full warnings before acting; the locator is not a waiver or summary. Filtered callers never receive unreadable ancestors' IDs, text or warnings; unavailable root/parent positions say `unavailable`. This version is comparable with outline and SQL for the same authored store. Refine the query or subtree if matches exceed the displayed page; do not claim all results were examined.

Read relevant hit rows as candidate sources and expand `path_template` with their IDs to open actual `nodes/UUID/node.md`; use root/parent IDs and shallow Outline Navigation for surrounding structure. A match is a candidate source, not a proven answer or authorization. No hits means this lexical query did not find evidence, not that the subject is absent. Try a more precise/different phrase or a structural SQL query when that distinction matters.

Rejection or a noncurrent result stops dependent reasoning. Inspect the authored store and rerun the complete check before retrying; never silently substitute stale results. Return the supported answer with source UUIDs/version and any remaining uncertainty to the original task.
