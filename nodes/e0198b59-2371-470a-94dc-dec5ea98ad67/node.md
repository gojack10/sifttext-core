---
id: "e0198b59-2371-470a-94dc-dec5ea98ad67"
name: "See Also"
micro_summary: "Explains explicit indexed references and how their reasons distinguish required dependencies from optional context."
status: "in_progress"
see_also: []
scope: "Use the native see_also field for indexed target/reason relationships, including dependencies, without confusing narrative citations with graph edges."
warnings: ""
---
Native `see_also` is a JSON array of `{"target":"UUID","reason":"Nonblank relationship"}`. It authors indexed graph edges. These can represent required dependencies, evidence or optional context; the reason and applicable procedure must make that distinction explicit.

The distinction matters:

- **Inline links** (in scope or crystallization) are part of the narrative. "This depends on Authentication Flow" — the link is load-bearing, the text does not make sense without it.
- **See also edges** record relationships in the native index. A reason such as "Required input before editing" describes a dependency; "Background comparison" describes optional context. A citation in the body does not create either edge.

When to use see_also:
- A required dependency or evidence source that needs an indexed relationship
- A related node in a different branch that shares context
- Background reading that informed this node's decisions
- A node that covers a similar topic from a different angle

When to add a narrative citation:
- The reference is part of the argument or reasoning
- The node's meaning is incomplete without the linked content
- You are citing a specific decision or dependency

Use both an explicit edge and a readable citation when the narrative needs the target and indexed discovery also matters. Read both endpoints, avoid redundant direct parent/child edges, and validate under Worktree Publication. SQLite Queries owns the `links` table, including incoming-edge lookup. Neither a saved edge nor a citation proves that a model read its target.

For the deeper philosophy of why links matter and how they bridge human intuition and language model depth, see [[Why Linking Matters|e60740b4-173a-4e76-865d-99aefc49faa2]].