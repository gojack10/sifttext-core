---
id: "f961a20f-1df5-4c0f-a263-9730e2a28aca"
name: "Outline Navigation"
micro_summary: "Guides branch discovery through shallow and full LEAN views while distinguishing generated or stale material from authored truth."
status: "in_progress"
see_also: []
scope: "Navigate a known branch with current shallow/full LEAN outlines and then read the nodes needed by the task."
warnings: ""
---
Input: a confirmed knowledge home or private worktree and a known node UUID. Use Leviathan Search or SQLite Queries first if identity is unknown. Set `store` to that working copy, not the code checkout.

```sh
sifttext outline "$store" 38a9fc79-8a84-494d-a962-2ec5134bc856
sifttext outline "$store" 38a9fc79-8a84-494d-a962-2ec5134bc856 full
```

The default is shallow; `full` requests the descendant view. The command refreshes native SQLite/outlines from current parseable authored files, including uncommitted drafts, and verifies the generated view's freshness before returning compact LEAN. This is not publication certification or a full-store Bend gate. LEAN is the compact transport format, not an edit language. Raw generated files are convenient for inspection but can be stale; use the command when freshness matters. The authored-store `version` is comparable with search and SQL: an opaque 64-hex `authored-merkle-v2` inventory identity, not a Git commit or JSON-file hash. Older cache/version formats invalidate once. Rows retain hierarchy, IDs, intact names/micro-summaries, statuses, child counts and applicable warnings/active reasons, with one `path_template`. In shallow coverage, `full_view_tokens` estimates the exact full CLI output for each expandable branch; leaves have no expansion estimate. Internal generated files retain richer verification metadata and are not the CLI contract.

Start shallow and expand the selected branch, not the whole tree by habit. Narrow very wide parents with existing search or SQL. Choose a branch using names, summaries, statuses, child counts and hierarchy. Read `nodes/UUID/node.md` for the actual scope, answer, evidence and continuation. Read references only when they supply a needed constraint or input; an outline is not execution permission. The entrypoint's selected required reads still apply; do not invent a fixed child count.

Cycles or missing relationships can leave SQL usable while an outline returns explicit `status:invalid_draft` where honest hierarchy cannot be provided. Inspect its structural diagnostic and native source; repair within authority and refresh. Do not mistake empty invalid coverage for a leaf. Malformed or unsafe source refuses rather than returning stale or partial success. For a node without a usable generated view, read its own `node.md` and child entries directly without inventing descendants. Do not invoke a standalone `check` or bypass safeguards; acceptance belongs to submit.

Return the selected owner, relevant source evidence and unresolved question to the original task. Stop expanding once sufficient context is available; expand again for a specific gap, not to prove the whole forest was read.
