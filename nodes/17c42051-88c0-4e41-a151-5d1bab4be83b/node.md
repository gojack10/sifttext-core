---
id: "17c42051-88c0-4e41-a151-5d1bab4be83b"
name: "Status"
micro_summary: "Explains the native lifecycle values and delegates their validation and publication mechanics to the local entry pipeline."
status: "resolved"
see_also: []
scope: "The status field and its six native lifecycle values, including required reasons and the distinction between a status and evidence."
warnings: ""
position: 3072
---
Status records a knowledge work item's lifecycle disposition. Official GitHub/bundled user-facing documentation is exempt from epistemic-state transitions and completion/reopening checks. Its stored metadata remains readable, but editing a maintained guide does not make it unfinished user work. Keep valid native metadata and structural relationships; the exemption does not apply to non-bundled customer or founder development nodes.

| Native value | Meaning | Required field |
| --- | --- | --- |
| not_examined | Identified work that is not yet examined | None |
| in_progress | Active work or a partial answer | None |
| resolved | The node's own completion condition is satisfied | A supporting result in the body |
| stuck | A named external condition blocks continuation | blocker |
| deferred | Deliberately postponed work | deferred_reason with a resume condition |
| discarded | A rejected approach or interpretation | discard_reason |

Submission applies native activation and parent reopening to non-exempt knowledge. Documentation neither reopens from active work beneath it nor blocks an ordinary parent's resolution merely because its stored status is active.

Choose status from evidence about the node's own claim.
