---
id: "b6107909-a33b-4c63-ab0d-351ba1a1b672"
name: "Knowledge Layout"
micro_summary: "Explains how repository locations, UUID folders, authored fields and ordered child relationships fit together for local knowledge navigation."
status: "resolved"
see_also: []
scope: "Locate the local knowledge home and understand its native identity, content and hierarchy before using retrieval or editing tools."
warnings: ""
---
## Locate the home

Use the knowledge home named on the command, normally ~/sifttext.

Inspect git status there before writing.

Keep it separate from the Core source checkout and the software installation.

## Layout

```text
nodes/UUID/node.md
nodes/UUID/children.order
nodes/UUID/children/UUID
nodes/UUID/artifacts/NAME
trees/ROOT_UUID
.sifttext/index.sqlite
```

The UUID identifies a node.

Parentage comes from child symlinks, and order comes from children.order.

Accepted knowledge belongs to one rooted tree.

Read node.md directly or follow the tree shortcut.

## Node artifacts

Task files belong under the artifacts directory of the deepest node that owns the work, in the durable knowledge home.

For a new owner, publish the knowledge node first, then place the files in its accepted home.

Verify that the artifact directory exists and that Git ignores the files.

A symlink inside artifacts is an alias, so editing through it edits the target.

No artifact is transported by knowledge Git or release updates.

Keep a separate authorized backup when one is needed.

## Metadata

Optional scope and warnings are strings.

see_also holds target/reason pairs whose targets exist in the same knowledge forest.

The status values and their required fields are defined in [[Status|17c42051-88c0-4e41-a151-5d1bab4be83b]].

## Derived state

.sifttext and outline files are derived views, not authored knowledge.

Refresh them with the installed commands rather than editing them.

Authored files are bounded at 1 MiB.

Submission enforces a 5000 o200k_base token ceiling on the content body.

Keep the native guards intact and decompose evidence rather than truncating it.

## Return

Use [[Retrieve SiftText Knowledge|526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8]] for a read task.

Use [[Worktree Publication|fb1e0c9f-8c04-4baf-91e6-4688856a14af]] before changing knowledge.
