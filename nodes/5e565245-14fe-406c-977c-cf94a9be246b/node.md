---
id: "5e565245-14fe-406c-977c-cf94a9be246b"
name: "Local Core Agent Entrypoint"
micro_summary: "Owns the selection of child skill reads and stop boundaries for agents using offline Local Core."
status: "in_progress"
see_also: []
scope: "Bootstrap an agent from this file into the local Core instruction pipeline without hosted access or prior SQL knowledge."
warnings: ""
---
Read this file first. Before interpreting stored knowledge, read the bundled SiftText Read procedure at `nodes/5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d/node.md`; use it to recover the user's actual question, relevant evidence, current understanding and uncertainty without requiring a fixed navigation sequence or exhaustive history. Read only the other direct-child instructions needed for the task, following their order in `children.order` and paths under `children/`: CLI Operations for command use, Knowledge Layout for source orientation, and Outline Navigation, Leviathan Search or SQLite Queries when their conditions apply. Ordinary file reads suffice; no database lookup is required to start. Reuse already-loaded unchanged instructions.

Before an authorized knowledge write, read the bundled SiftText Commit procedure at `nodes/c72d552d-3499-445b-a8d5-05d0ff7824f2/node.md` and follow its required pre-write reads. Preserve Worktree Publication's fixed-baseline validation, reconciliation and authority checks; a read or a write request does not grant permission. Tell the human the current step and expected check. Return evidence to the original task, not an invented next assignment.

Before saving or naming the location of a task file, read Knowledge Layout's Node artifacts section and follow its durable-home placement and preservation checks. Knowledge worktree cleanup is not artifact backup.

The human may pause or reshape this route. Clarify ambiguous overrides, preserve unmentioned work, and state the revised next step. Never silently waive permissions or checks. On missing input, unsafe paths or a failed check, retain the draft and report the exact blocker; resume at that check after an authorized repair.
