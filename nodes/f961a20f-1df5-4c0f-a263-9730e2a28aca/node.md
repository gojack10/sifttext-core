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

The default is shallow; `full` requests the descendant view. The command refreshes native SQLite/outlines, verifies the complete generated manifest against the authored source, then returns captured LEAN text. LEAN is the compact transport format, not an edit language. Raw generated files are convenient for inspection but can be stale; use the command when freshness matters. `source_version_scope:outline_view` identifies `source_version` as the fingerprint of this displayed view, not the authored-store `version` returned by search or SQL. Different views can have different fingerprints without being stale; the command performs freshness validation before returning.

Choose a branch using names, summaries, statuses and hierarchy. Read `nodes/UUID/node.md` for the actual scope, answer, evidence and continuation. Read references only when they supply a needed constraint or input; an outline is not execution permission. The entrypoint pipeline's mandatory child reads still apply.

For a node with no generated outline, read its own `node.md` and child entries directly; do not invent missing descendants. On rejection, inspect the authored files and the complete candidate check. Do not rely on an older outline or remove safeguards to get a view.

Return the selected owner, relevant source evidence and unresolved question to the original task. Stop expanding once sufficient context is available; expand again for a specific gap, not to prove the whole forest was read.
