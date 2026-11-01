# Child positions and migration

Relationships remain symlinks at `nodes/PARENT/children/CHILD`. Converted nodes
carry an unsigned integer `position` in their Markdown front matter instead of
sharing a parent's `children.order` file. Readers enumerate child links and sort
by `(position, UUID)`. Concurrent appends may choose the same position; the UUID
makes their displayed order deterministic. No merge driver or model is involved.

`sifttext create` under a converted parent writes only the new node's files and
its relationship link. It appends at the maximum sibling position plus 1024.
To change display order, edit that child's position; gaps allow ordinary inserts.
Legacy parents still use their original list until explicitly converted.

## Authorized conversion

Upgrade all participating local, hosted and guest Core readers/writers first.
Older Core cannot read converted homes; keep the managed candidate pinned until
the official release supports this format. Finish retained legacy contributions
before conversion so they do not modify a list the migration deletes.

Allocate a normal private worktree, then run `sifttext migrate-order WORKTREE`.
The deterministic converter gives every child its existing ordinal times 1024,
gives roots position zero, and removes the shared lists. It preserves bodies,
other metadata, relationships, statuses, references and the displayed sequence.
Commit and submit through normal native publication using the original base.
The converter itself never advances accepted main or rewrites history.

Position-only edits do not buy a new semantic check. Native graph/status and
publication checks still run, including on the final Git-combined candidate.
Ordinary conflicting body edits and moves still use native Git/Core recovery.

## Rollback

Before reverting Core, allocate a draft with the new Core and run
`sifttext migrate-order WORKTREE --legacy`. Commit and submit the regenerated
lists, sync the accepted result, then select the previous packages. This retains
all later accepted knowledge; never reset main to the pre-migration revision.
