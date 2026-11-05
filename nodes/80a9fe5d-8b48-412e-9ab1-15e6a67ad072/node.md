---
id: "80a9fe5d-8b48-412e-9ab1-15e6a67ad072"
name: "Child Positions and Migration"
micro_summary: "Explains position-based child order and automatic one-time home conversion."
status: "resolved"
see_also: [{"target":"fb1e0c9f-8c04-4baf-91e6-4688856a14af","reason":"Publish an authorized ordering conversion through a private draft."}]
scope: "Maintain deterministic child order and recover an interrupted automatic format migration."
position: 1024
---
# Child positions and migration

Relationships are symlinks at `nodes/PARENT/children/CHILD`. Each node carries an unsigned integer `position` in its front matter. Readers enumerate child links and sort by `(position, UUID)`. Concurrent appends can choose the same position; UUID breaks the tie deterministically.

`sifttext create` writes the new node and its relationship link. It appends at the maximum sibling position plus 1024, creating the parent's child directory when adding its first child. New leaves need no child directory or `.keep` file. To change display order, edit the child's position in a private draft and publish through [Worktree Publication](../fb1e0c9f-8c04-4baf-91e6-4688856a14af/node.md).

## Automatic conversion

The incoming release converts an older home during update preparation or its first home command. Users do not run a separate conversion command. The isolated importer supports homes that skipped earlier releases; normal readers and writers use positions only.

Core allocates a native draft under the platform temporary directory. It replaces each legacy child list with positions preserving its displayed sequence, retains existing positions elsewhere, and uses zero when a node has no placement rank. It then removes the legacy lists and converts wiki-style body references to relative Markdown links. UUIDs, labels, statuses, relationships and other authored content are retained. Structured `see_also` data is unchanged.

The ordering and link changes are committed and submitted through normal publication. The migration carries its scoped findings waiver; actual findings remain findings, and structural checks still apply. It does not claim that an earlier semantic review covered rewritten text. This does not authorize agents to waive findings for unrelated submissions.

After publication, reader completion and draft removal succeed, Core records top-level `order_format_version: 1` and `link_format_version: 1` in `.sifttext/config.json`, preserving unrelated settings. These are completion observations, not update permissions. Another installation uses the same one-time route.

## Interrupted conversion

Keep `.sifttext/format-migration.json`, the reported draft and native save journals. Retry the original command. A refused publication or unfinished reader does not receive completion markers. If accepted knowledge changed concurrently, reconcile the retained draft rather than resetting either history.

Other owners' linked drafts are not silently rewritten. Preserve their work and reconcile it against the migrated accepted home before submission. Do not recreate legacy lists to accommodate an older writer.

There is no `migrate-order --legacy` mode. Preserve Git history and use a position-aware release when selecting another binary; never reset accepted knowledge to undo a format change.
