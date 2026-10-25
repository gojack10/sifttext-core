---
id: "da56a037-af73-469c-a941-a2b7c5f290b7"
name: "What is a Node"
micro_summary: "Anatomy reference for the fundamental unit, naming every field and delegating each field's detailed meaning to its own child."
status: "in_progress"
see_also: []
scope: "The fundamental unit. What a node is, what its fields are. Children of this node explain each field in detail."
warnings: ""
---
A node is the fundamental unit of a tree. It holds one piece of your thinking.

Every node has a name (2-5 words identifying what it is about) and a set of fields:

- **Scope** — what this node is about. The boundary of what belongs here.
- **Content** — what is known. Conclusions, decisions, working notes.
- **Micro-summary** — stable responsibility that helps readers choose this node.
- **Status** — not examined, in progress, resolved, stuck, deferred, or discarded; required reasons and evidence follow Knowledge Layout.
- **Warnings** — cautions to check when reading the node; no automatic agent delivery is assumed.
- **See Also** — explicit target/reason graph references, including useful dependencies.
- **Children** — sub-nodes represented by child symlinks and `children.order`, not metadata fields.
- **Links** — indexed `see_also` edges; narrative citations remain readable pointers.

Native metadata has no priority or ruled_out field. Preserve distinct rejected approaches as discarded nodes with reasons and evidence; a small qualification can stay in its owning body. Knowledge Layout owns the complete supported grammar.

Each field is explained in detail in the children of this node.