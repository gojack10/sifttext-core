---
id: "5e565245-14fe-406c-977c-cf94a9be246b"
name: "Local Core Agent Entrypoint"
micro_summary: "Owns the selection of child skill reads and stop boundaries for agents using offline Local Core."
status: "in_progress"
see_also: []
scope: "Bootstrap an agent from this file into the local Core instruction pipeline without hosted access or prior SQL knowledge."
warnings: ""
---
Read every direct child's `node.md` in the order recorded in this directory's `children.order`, following the matching symlinks in `children/`. Ordinary file reads suffice; no database lookup is required. Reuse an already-loaded unchanged pipeline.

Tell the human the current step and expected check. After the reads, follow CLI Operations for setup, Knowledge Layout for source orientation, then the relevant retrieval procedure. Before any edit follow Worktree Publication; return evidence to the original task, not an invented next assignment.

The human may pause or reshape this route. Clarify ambiguous overrides, preserve unmentioned work, and state the revised next step. Never silently waive permissions or checks. On missing input, unsafe paths or a failed check, retain the draft and report the exact blocker; resume at that check after an authorized repair.
