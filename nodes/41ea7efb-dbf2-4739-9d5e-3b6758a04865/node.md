---
id: "41ea7efb-dbf2-4739-9d5e-3b6758a04865"
name: "Priority"
micro_summary: "Explains relative urgency as authored context for human or agent work selection, not an automatic Core scheduler."
status: "in_progress"
see_also: []
scope: "How to express relative urgency in local knowledge without assuming automatic prioritization or a dedicated indexed field."
warnings: ""
---
Priority indicates how urgent or important a node is relative to its siblings. It helps you and language models decide what to work on next.

- **High** — this needs attention now. It is blocking other work, has a deadline, or is critical to the tree's purpose.
- **Medium** — important but not urgent. Should be addressed in the normal course of work.
- **Low** — nice to have. Can wait. Will not block anything if deferred.

Priority is relative, not absolute. A "high" priority in a low-stakes hobby tree is different from a "high" priority in a production system tree. Context matters.

In local Core, priority is guidance you author in a node's scope or body for a human or separately configured agent to consider. Core does not choose your next task, supply an assistant, or order lexical search by priority. The three levels above are a useful writing convention, not a checked enum or dedicated retrieval field.