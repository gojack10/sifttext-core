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
nodes/UUID/parent
nodes/UUID/links-to/UUID
nodes/UUID/linked-from/UUID
nodes/UUID/artifacts/NAME
trees/ROOT_UUID
.sifttext/index.sqlite
.sifttext/config.json
```

The UUID identifies a node.

Parentage comes from child symlinks. Display order comes from each child node's `position` in its front matter. Older homes are converted automatically during update preparation or first use. For ordering edits or migration recovery, read [Child Positions and Migration](../80a9fe5d-8b48-412e-9ab1-15e6a67ad072/node.md).

A leaf may omit its `children` directory; an absent or empty directory means zero children. Create that directory when adding the first child link. No `children/.keep` file is required. Existing empty markers remain accepted, and this change needs no home migration. An existing child container must still be a real directory, and its relationship links remain validated.

Earlier releases require child directories even for leaves. Update every application or service reading a shared home before publishing marker-free leaves to it. Root containers and their existing markers are unchanged.

Write body references as normal Markdown links. Resolve a relative target from the containing file: from `nodes/SOURCE/node.md`, link a sibling with `[Name](../TARGET/node.md)`. Preserve the target UUID and meaningful label. Structured front-matter `see_also` records remain data, not Markdown links.

The normal knowledge home generates three navigation paths. `parent` opens the parent node. `links-to/` and `linked-from/` show cross-references from relative Markdown node links and `see_also`. These shortcuts are local and rebuildable. Edit the source node or its authored child links to change a relationship. Managed updates prepare the shortcuts once and record `node_links_format_version` in `.sifttext/config.json`.

Use `sifttext outline STORE UUID` for the node and its direct children. Add `full` for the whole subtree. The command reads the derived SQLite index and returns LEAN.

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

Use [Status](../17c42051-88c0-4e41-a151-5d1bab4be83b/node.md) to choose a status and supply its required fields.

## Local state

Run `sifttext index STORE` to prepare `.sifttext/index.sqlite` from the knowledge files. During conversion from a file-backed index, this removes old outline files whose bytes match the recorded hashes.

Preserve `.sifttext/config.json` during cache repairs because it stores the user's update permissions and sharing choices.

## Knowledge size limits

Authored files are bounded at 1 MiB.

The content body must fit within 5000 tokens measured with `o200k_base`.

Keep the native guards intact and decompose evidence rather than truncating it.

## Return

Use [Retrieve SiftText Knowledge](../526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8/node.md) for a read task.

Use [Worktree Publication](../fb1e0c9f-8c04-4baf-91e6-4688856a14af/node.md) before changing knowledge.
