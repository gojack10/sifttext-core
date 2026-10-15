---
id: "b3ed2d68-5a2d-4432-adb8-f5507861e203"
name: "Cross-Tree Links"
micro_summary: "Connects domains inside one local forest without a hosted tree-container identifier."
status: "in_progress"
see_also: []
scope: "Express an authorized relationship between nodes in different rooted trees of the same knowledge repository."
warnings: "A link grants no access to another repository; all native edge targets must exist in the selected forest."
---
Local trees share a single `nodes/UUID/` namespace. A `see_also` edge uses the target node UUID and a nonblank reason, whether that target is in the same rooted tree or another tree. There is no `@tree_id` suffix or hosted container-ID lookup in the native graph grammar. Knowledge Layout owns that grammar; the sibling named Inline Links explains narrative citations versus indexed edges.

Read both endpoints before adding a relationship. Record why the connection matters, for example a staffing dependency between product and hiring knowledge. Avoid duplicating the target's answer in the source.

A separate knowledge repository has separate custody and permissions. No native edge crosses into it automatically. If an authorized external artifact is needed, identify its source/location and access boundary in prose; report an unavailable target rather than inventing a local node or copying private knowledge without permission. References are not access grants.
