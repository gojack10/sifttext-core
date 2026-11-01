---
id: "2d5dba68-1346-4704-a948-001e4b271013"
name: "Deferred"
micro_summary: "Status value for consciously paused but already scoped work, and the reason to record so a later agent knows when to revisit."
status: "resolved"
see_also: []
scope: "Status value. Intentionally set aside for later. Not abandoned, not active."
warnings: ""
position: 4096
---
Use deferred when deliberately postponing scoped work.

Supply deferred_reason with the reason for the postponement and the condition for resuming.

Keep existing findings available so the work can continue when that condition is met.
