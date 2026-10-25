---
id: "2d5dba68-1346-4704-a948-001e4b271013"
name: "Deferred"
micro_summary: "Status value for consciously paused but already scoped work, and the reason to record so a later agent knows when to revisit."
status: "resolved"
see_also: []
scope: "Status value. Intentionally set aside for later. Not abandoned, not active."
warnings: ""
---
Deferred means this node has been intentionally set aside. It is not abandoned. It is not forgotten. It is parked until the right time.

Use deferred when:
- The node depends on something else being resolved first
- It is not a priority right now but will be later
- You need more information that is not yet available

Deferred is different from not_examined. A not_examined node was never started. A deferred node was started or at least scoped, and a conscious decision was made to pause it.

When you defer a node, it helps to note why in the content: "Deferred until Q3 when the API is stable" gives future-you (or a language model) the context to know when to revisit.