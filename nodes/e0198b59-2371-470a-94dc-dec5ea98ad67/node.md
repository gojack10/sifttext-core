---
id: "e0198b59-2371-470a-94dc-dec5ea98ad67"
name: "See Also"
micro_summary: "Explains explicit indexed references and how their reasons distinguish required dependencies from optional context."
status: "resolved"
see_also: []
scope: "Use the native see_also field for indexed target/reason relationships, including dependencies, without confusing narrative citations with graph edges."
warnings: ""
---
The see_also array records indexed relationships between existing nodes.

```json
[{"target":"UUID","reason":"Why this connection matters"}]
```

Use the reason to explain whether the target supplies a required dependency or optional context.

Read both endpoints and confirm that the target exists before proposing the edge.

Publish approved edits through [[Worktree Publication|fb1e0c9f-8c04-4baf-91e6-4688856a14af]].
