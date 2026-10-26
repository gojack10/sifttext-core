---
id: "17c42051-88c0-4e41-a151-5d1bab4be83b"
name: "Status"
micro_summary: "Explains the native lifecycle values and delegates their validation and publication mechanics to the local entry pipeline."
status: "in_progress"
see_also: []
scope: "The status field and its six native lifecycle values, including required reasons and the distinction between a status and evidence."
warnings: ""
---
Status records a node's lifecycle disposition.

| Native value | Meaning | Required field |
| --- | --- | --- |
| not_examined | Identified work that is not yet examined | None |
| in_progress | Active work or a partial answer | None |
| resolved | The node's own completion condition is satisfied | A supporting result in the body |
| stuck | A named external condition blocks continuation | blocker |
| deferred | Deliberately postponed work | deferred_reason with a resume condition |
| discarded | A rejected approach or interpretation | discard_reason |

Submission applies native activation and parent reopening.

Choose status from evidence about the node's own claim.
