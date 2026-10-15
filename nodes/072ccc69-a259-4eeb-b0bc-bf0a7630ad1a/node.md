---
id: "072ccc69-a259-4eeb-b0bc-bf0a7630ad1a"
name: "Inline Links"
micro_summary: "Distinguishes readable narrative citations from explicit native graph edges."
status: "in_progress"
see_also: []
scope: "Cite local nodes in prose without claiming that saving text creates indexed references or backlinks."
warnings: "Prose citations do not author graph edges; use a validated see_also target/reason for an indexed relationship."
---
Use `[[Name|UUID]]` as a readable citation in the Markdown body or scope. The UUID identifies `nodes/UUID/node.md` in the selected knowledge repository; open that file to read the target. Resolve actual targets through local search or SQL before citing them. Do not invent UUIDs or use examples as real references.

A citation can be load-bearing: a dependency, evidence source or decision without which the text is incomplete. Background context can also be recorded as a reasoned explicit reference. The native implementation does not parse prose citations into edges, add backlinks on save, or provide a web link picker.

For an indexed graph relationship, edit the source node's JSON-valued `see_also` array:

```json
[{"target":"5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d","reason":"Reading procedure used before this authoring task."}]
```

Use that example only when its actual relationship is warranted. Read both endpoints, avoid self/duplicate/direct parent-child edges, then validate and publish under Worktree Publication. SQLite Queries exposes authored edges through `links`; querying target as well as source finds incoming references. A source change does not write the target file.
