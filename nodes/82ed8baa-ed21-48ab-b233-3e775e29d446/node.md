---
id: "82ed8baa-ed21-48ab-b233-3e775e29d446"
name: "Links"
micro_summary: "Explains how explicit native references connect knowledge without duplicating its answers."
status: "in_progress"
see_also: [{"target":"ab82fdf1-6a71-4e09-bffe-ed6446eadc9a","reason":"Current ownership review preserves useful findings beneath rejected approaches while keeping them discoverable."}]
scope: "Distinguish local narrative citations, indexed see_also edges and structural parentage."
warnings: "Saving a prose citation does not create an edge or backlink."
---
## Current understanding

Links express a dependency, source or useful connection without duplicating content. Native indexed edges live only in the JSON-valued `see_also` metadata array: each entry has a target node UUID and a nonblank reason. Every target must exist in the same knowledge forest; self and duplicate references are rejected.

Markdown `[[Name|UUID]]` citations in scope/body are readable pointers, not parsed graph edges. Use the target's ordinary `nodes/UUID/node.md` file for reading. The same UUID-based edge format works across local tree roots; no hosted container ID is needed. SQLite Queries owns the actual `links(source,target,reason,position)` discovery surface. Incoming edges are found by target, not separately authored into the target file.

Parent/child symlinks and `children.order` own hierarchy. Do not duplicate those direct relationships in `see_also`. Add only useful connections that structural navigation does not already express, and explain their purpose.

Rejected approaches are represented by discarded nodes with reasons and evidence, not an unsupported `ruled_out` metadata field. Useful discoveries may remain beneath the rejected approach that produced them; connect them to continuing work when needed for discovery. The current Review Origin and Response Coverage method replaces the earlier blanket exclusion of such ancestry. Read both endpoints before editing references, then validate the complete candidate and publish under Worktree Publication.

## Preserved earlier guidance

The complete earlier body below is historical, not additional operative guidance. Its blanket ancestry restriction was replaced by the current ownership review above; native reference mechanics were retained.

Links express a dependency, source or useful connection without duplicating content. Native indexed edges live only in the JSON-valued `see_also` metadata array: each entry has a target node UUID and a nonblank reason. Every target must exist in the same knowledge forest; self and duplicate references are rejected.

Markdown `[[Name|UUID]]` citations in scope/body are readable pointers, not parsed graph edges. Use the target's ordinary `nodes/UUID/node.md` file for reading. The same UUID-based edge format works across local tree roots; no hosted container ID is needed. SQLite Queries owns the actual `links(source,target,reason,position)` discovery surface. Incoming edges are found by target, not separately authored into the target file.

Parent/child symlinks and `children.order` own hierarchy. Do not duplicate those direct relationships in `see_also`. Add only useful connections that structural navigation does not already express, and explain their purpose.

Rejected approaches are represented by discarded nodes with reasons and evidence, not an unsupported `ruled_out` metadata field. Keep useful discoveries outside discarded ancestry. Read both endpoints before editing references, then validate the complete candidate and publish under Worktree Publication.
