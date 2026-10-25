---
id: "17c42051-88c0-4e41-a151-5d1bab4be83b"
name: "Status"
micro_summary: "Explains the native lifecycle values and delegates their validation and publication mechanics to the local entry pipeline."
status: "in_progress"
see_also: []
scope: "The status field and its six native lifecycle values, including required reasons and the distinction between a status and evidence."
warnings: ""
---
Every node has a status that indicates where it is in its lifecycle. Status helps you and any language model understand what still needs work, what is done, and what has been set aside.

The six native statuses describe a lifecycle:

- **Not Examined** — the starting state. This node exists but has not been worked on yet.
- **In Progress** — actively being worked on. Scope is defined, content is evolving.
- **Resolved** — the node's own acceptance claim is proven; its nonblank body records the answer and evidence, and it has no active direct child.
- **Stuck** — active work blocked by a named external condition; author a nonblank `blocker`.
- **Deferred** — intentionally set aside for later. Not abandoned, but not active.
- **Discarded** — considered and rejected. Kept for the historical record of what was tried.

The children explain the other status concepts. Knowledge Layout owns the complete native grammar: deferred requires `deferred_reason` and a resume condition; discarded requires `discard_reason`. Worktree Publication owns explicit status requests and fixed-baseline normalization/checks. Status is an authored claim, not proof that Core or an agent completed the work.