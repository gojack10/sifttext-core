---
id: "b6107909-a33b-4c63-ab0d-351ba1a1b672"
name: "Knowledge Layout"
micro_summary: "Locates knowledge files and identifies the local files that can be rebuilt."
status: "resolved"
see_also: []
scope: "Find knowledge files and identify rebuildable local views."
warnings: ""
position: 2048
---
## Locate the home

Use the knowledge home named on the command, normally ~/sifttext.

Inspect git status there before writing.

Keep it separate from the Core source checkout and the software installation.

## Layout

```text
nodes/UUID/node.md
nodes/UUID/children/UUID
nodes/UUID/outline.lean
nodes/UUID/outline.full.lean
nodes/UUID/artifacts/NAME
trees/ROOT_UUID
.sifttext/index.sqlite
.sifttext/config.json
```

The UUID identifies a node.

Parentage comes from child symlinks. Display order comes from each child node's `position` in its front matter. Legacy homes may still use `children.order` until explicitly migrated; upgrade every writer before converting one.

Generated views live at `nodes/<id>/outline.lean` (the node and its direct children) and `nodes/<id>/outline.full.lean` (the whole subtree). Prefer ordinary file tools on them. The outline command fast-tracks the same files and prepends `outline-file` with the selected file's full absolute path. These rebuildable views may need generation before they exist.

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

Each `see_also` entry names another node in this knowledge home and explains its relevance.

Use [[Status|17c42051-88c0-4e41-a151-5d1bab4be83b]] to choose a status and supply its required fields.

## Local state

Use the installed commands to rebuild `.sifttext/index.sqlite` and generated outline files from the knowledge files.

Preserve `.sifttext/config.json` during cache repairs because it stores the user's update permissions and sharing choices.

## Knowledge size limits

Authored files are bounded at 1 MiB.

The content body must fit within 5000 tokens measured with `o200k_base`.

Keep the native guards intact and decompose evidence rather than truncating it.

## Return

Use [[Retrieve SiftText Knowledge|526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8]] for a read task.

Use [[Worktree Publication|fb1e0c9f-8c04-4baf-91e6-4688856a14af]] before changing knowledge.
