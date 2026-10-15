---
id: "fe634065-36f1-4214-a131-7d36a587fba2"
name: "Ruled Out Links"
micro_summary: "Preserves connections to rejected approaches without using an unsupported metadata field."
status: "in_progress"
see_also: []
scope: "Reference a discarded local approach and its evidence so future readers do not repeat it unknowingly."
warnings: "Native metadata rejects ruled_out; use a discarded node and a reasoned see_also edge when a graph reference is needed."
---
For a distinct considered-and-rejected approach, retain its node with `status:"discarded"`, a nonblank `discard_reason` and a body stating whether rejection came from an actual trial or reasoning. Preserve its evidence and the conditions that would justify revisiting it. A human may reopen it; rejection is not an immutable truth.

A live outcome can reference that approach using `see_also` with its actual UUID and a reason explaining the relationship. A narrative citation can explain the rejection but does not author a graph edge. Do not add redundant parent/child references. The sibling named Inline Links owns that distinction.

Keep independently useful measurements under a non-discarded owner rather than burying them beneath the rejected approach. Read the relevant attempt before proposing it again; different conditions can reopen the question without erasing meaningful history.
