---
id: "f2310937-f481-48e4-9aad-8b2486cafc3d"
name: "Reconciliation"
micro_summary: "Expected post-Persistence pass over affected parents/connectivity. Three modes: A sparse inline reference only when prose genuinely needs it, B section rewrite for stale sections, C see_also addition for deferred cross-refs. Never add child TOCs."
status: "in_progress"
see_also: []
scope: "Phase 4 of SiftText Commit. After Persistence lands new children, update affected parent crystallizations and connectivity. Runs as an expected pass on every commit, not as conditional refinement."
warnings: ""
---
Phase 4 of SiftText Commit. After Persistence lands creates/edits/moves/links, affected parents and connected nodes may have stale crystallizations or connectivity because the write changed assumptions, prose, status, or cross-references. Reconciliation closes that gap as an expected pass — not a conditional refinement. Treating it as expected is what prevents drift; treating it as optional is how trees rot.

For each parent that gained a new child or whose existing crystallization is now stale, pick one mode:

- Mode A — Sparse inline reference. The parent's existing crystallization is still correct; it just needs a link when the new child is genuinely referenced in the parent's prose narrative. Routine children are discoverable via native child symlinks/order files and SQLite Queries — never enumerate them as a duplicate prose directory. Add a narrative citation only where reasoning needs it, and an explicit `see_also` target/reason for a required graph edge; cross-branch distance alone does not justify a link.
- Mode B — Section rewrite. The parent has an existing section that is now stale because of what Persistence wrote. Edit that Markdown body section with ordinary file tools, preserving faithful content; replace only the stale specifics.
- Mode C — see_also addition. No crystallization edit. Read both endpoints, then edit the JSON-valued `see_also` array with the target UUID and a nonblank reason. Used when Persistence deferred cross-refs that should exist but were out of scope under Persistence's strict cross-ref policy.

After changes, repeat native normalization/checks and compare actual references and statuses; report which parents were touched and why. Continue to the sibling named Verify. If the needed semantic repair exceeds approval, stop dependent work and request the missing permission.