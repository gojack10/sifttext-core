---
id: "82ed8baa-ed21-48ab-b233-3e775e29d446"
name: "Links"
micro_summary: "Explains how explicit native references connect knowledge without duplicating its answers."
status: "in_progress"
see_also: []
scope: "Distinguish local narrative citations, indexed see_also edges and structural parentage."
warnings: "Saving a prose citation does not create an edge or backlink."
---
Links express a dependency, source or useful connection without duplicating content. Native indexed edges live only in the JSON-valued `see_also` metadata array: each entry has a target node UUID and a nonblank reason. Every target must exist in the same knowledge forest; self and duplicate references are rejected.

Markdown `[[Name|UUID]]` citations in scope/body are readable pointers, not parsed graph edges. Use the target's ordinary `nodes/UUID/node.md` file for reading. The same UUID-based edge format works across local tree roots; no hosted container ID is needed. SQLite Queries owns the actual `links(source,target,reason,position)` discovery surface. Incoming edges are found by target, not separately authored into the target file.

Parent/child symlinks and `children.order` own hierarchy. Do not duplicate those direct relationships in `see_also`. Add only useful connections that structural navigation does not already express, and explain their purpose.

Rejected approaches are represented by discarded nodes with reasons and evidence, not an unsupported `ruled_out` metadata field. Keep useful discoveries outside discarded ancestry. Read both endpoints before editing references, then validate the complete candidate and publish under Worktree Publication.
