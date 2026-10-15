---
id: "acfc85d8-04b8-47b8-90de-6a6443bf41e2"
name: "Warnings"
micro_summary: "Keeps actionable cautions in the authored metadata while leaving detailed rationale with its owning node."
status: "in_progress"
see_also: []
scope: "The warnings field. Cautions for anyone reading or working on this node. Safety rails and gotchas."
warnings: ""
---
Warnings are cautions in the JSON-valued `warnings` metadata string near the start of `node.md`. Read them before acting on the body. Ordinary full-file reads and `sifttext read` include them; an excerpt may omit them. Core does not automatically deliver them to a model or verify that they were followed.

Use warnings for:
- Constraints that are easy to miss ("this endpoint has a rate limit of 10 req/s")
- Dependencies that must be checked first ("read node X before modifying this")
- Gotchas from past experience ("the last time someone changed this, Y broke")
- Safety rails ("do NOT delete this data without a backup")

Humans and separately configured file-capable agents can inspect warnings in the local file. Core supplies no web UI or assistant. A well-placed warning can help prevent repeated mistakes, but stored text is not behavioral proof.

Keep warnings concise and actionable. A warning should tell the reader what to do or not do, not explain the full history. Link to another node if the backstory matters.