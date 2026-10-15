---
id: "4093b6ab-3f07-473c-a496-d89a262f97a6"
name: "Not Examined"
micro_summary: "Status value for identified but unstarted work, the placeholder created while decomposing a problem into children."
status: "resolved"
see_also: []
scope: "Status value. The starting state. This node exists but has not been worked on yet."
warnings: ""
---
Not examined is the default status when a node is created. It means: this node exists, it has a name and probably a scope, but nobody has done real work on it yet.

A not_examined node is a placeholder for future work. It marks a piece of the problem that has been identified but not explored. It is common to create several not_examined children when decomposing a problem, then work through them one at a time.

Move to in_progress when you start working on the node: researching, writing, thinking through the scope.