---
id: "d00ab6fd-b475-49ef-b37a-812807879308"
name: "Dialogue"
micro_summary: "Makes proposed local tree changes and their authority inspectable before persistence."
status: "in_progress"
see_also: []
scope: "Recover meaningful changes and prepare an exact local tree delta under the current approval or standing-write boundary."
warnings: "Do not use the write gate as a checkbox. After reading this child, reconsider the planned write and inspect any earlier refusal before retrying.\n  Do not collapse distinct rejected attempts into prose blobs. Native metadata has no `ruled_out`; meaningful rejected paths become discarded nodes with reasons.\n  Do not render an entire subtree as if it is new. Show only affected areas plus enough existing context, with operation marker left and status marker right.\na plausible giant delta is not informed approval; scanning or rubber-stamp impulse means the response failed.\na grouped operation inventory or schematic is not a canonical SiftText tree delta."
---
## Applies to Any Tree Write

This child is part of the mandatory pre-write pull. Its discipline applies to any ideation write, not only end-of-session commits. For a targeted point fix, the tree delta may be tiny, but the agent still must reconstruct what it intends to change before writing and inspect any earlier refusal before an authorized retry.

Phase 0/1 of full SiftText Commit. This is no longer a loose pre-Witness chat. It is the main user-facing reconstruction phase: conversationally rebuild the session's explored search path as a moldable Unix-style tree delta.

## Deep Scan First

For a retrospective, inspect the full supplied conversation or source range through the lens of **probe topology**. For a focused change, inspect its relevant source and affected context. Look for:

- Distinct probes/attempts that happened, including shots in the dark
- Which attempts were lateral alternatives versus downstream of a discovery
- Which attempts failed completely and should become discarded nodes
- Which attempts resolved a sub-question or produced information that unlocked children
- Slow assumption shifts, implicit decisions, and paths discussed early then abandoned
- Multi-step reasoning chains whose intermediate steps would be expensive to reconstruct

Do not omit meaningful earlier developments from a retrospective or collapse distinct probes into one abstract summary.

Preserve relevant human wording exactly with its context and role: question, example, proposal, instruction, approval or correction. Mark excerpt omissions and attribute recollections to their reporter. A peer-generated assignment retains that authorship; distinguish it from a human requirement, your interpretation and an observed result.

## The Lens: Reconstruct the Terrain

The goal is not "what should we summarize?" The goal is:

> What tree delta preserves the terrain we actually walked?

A discarded node means a distinct attempted path died and future agents should see that boundary in outline view. A resolved node means an attempt produced a usable answer, discovery, or win. If that discovery enabled further attempts, those attempts become children. If another probe was lateral/orthogonal, it becomes a sibling.

Do not compress distinct failed probes into a prose blob. A small qualification can stay in its owning body; distinct rejected attempts belong in the tree as discarded nodes.

## Decision Comprehension Gate

Before presenting anything, classify whether the human faces a real choice. Prior approval, existing law, or conclusive evidence means no new decision and no reapproval question.

For a real decision, present the problem, the exact ask, the canonical delta or option deltas, the practical consequences and what remains unchanged, the recommendation, and one explicit question. The human must be able to state the problem, the choice, what changes under each option, and what stays unchanged.

A plausible giant delta is not informed approval. A grouped operation inventory or schematic is not a canonical SiftText tree delta. Confusion, scanning, or rubber-stamp impulse means the response failed; stop and restate the problem and exact choice. When approval is required, treat replies as modifications, objections, corrections, omissions or clarifications until explicit authorization. An explicit standing write grant covers reviewed changes within its scope without another approval; discussion or silence grants none. No tree writes happen during Dialogue; return the authorized delta to Persistence.

## Unix Tree Delta Format

Render affected areas only: enough existing context for orientation, not the whole tree. Operation markers go on the left; status markers go on the right. If a node has no operation marker, it is existing unchanged context and should show only its status.

Status markers:

- `[~]` existing/in-progress
- `[ ]` not examined
- `[✓]` resolved
- `[x]` discarded
- `[=]` deferred
- `[/]` stuck

Operation markers:

- `[+]` create node
- `[!]` update existing node content/vitals/scope/status
- `[→]` move existing node
- `[←]` add link
- `[-]` delete node

For every link actually being added or changed, show `[←]`, the source node and field, the target, and a short relationship description. This also applies to links embedded in newly created nodes: annotate the create entry with the link addition. Do not hide these behind “references will be linked later.” Do not add links merely to satisfy the presentation requirement. `[→]` denotes a node move, not a link. Existing links that remain unchanged are retained context, not new operations.

Examples:

```text
[~] Existing in-progress context node, unchanged
[ ] Existing not-examined context node, unchanged
[x] Existing discarded context node, unchanged
[!][~] Existing in-progress node to update
[!][✓] Existing resolved node to update
[+][ ] New not-examined node
[+][x] New discarded probe node
[+][✓] New resolved discovery/win node
[→][~] Move existing in-progress node
[←][x] Add link to existing discarded node
[-][x] Delete existing discarded node
```

For each changed node, display its concise name and stable-purpose micro-summary, with explicit answer, alternative and independently testable work children required by the ownership test in Forcing Pass. Distinguish the micro-summary from labelled crystallization, blocker, warning and evidence notes. Do not collapse a question/answer pair merely because it has one child. Display an existing unchanged micro as context, not as an implied write. Read and apply the canonical naming, structure and field contracts through Forcing Pass before presentation.

When describing a proposed node inline, include terse action notes such as `Add cryst: ...`, `Add warning: ...`, `Discard reason: ...`, or `Update scope: ...`. Keep it compact enough for the human to mold.

### One-Shot Example

```text
[~] FooBar Exports
│   micro: Makes report delivery reliable without losing existing data.
│
├── [!][✓] Export Contract
│   Update micro: Separates the agreed output approach from proof
│                 that an implementation meets its limits.
│   └── [+][✓] Select Streaming
│       Add micro: Records the tradeoff behind incremental output
│                  instead of retaining the whole report in memory.
│       Add cryst: Human accepted streaming; operational proof is separate.
│
└── [+][ ] Large-Export Proof
    Add micro: Tests the resource boundary on a complete large report.
    Add cryst: Pass requires correct output within the agreed memory limit;
               no trial evidence exists yet.
```

This example is fictional. Its parent and existing contract are context, not instructions to create those nodes. Changed-node markers describe the intended effective status after the proposed writes.

## STOP Condition

When approval is required, stay in dialogue until the human explicitly authorizes the stated delta. Under an explicit standing write grant, proceed with the reviewed in-scope delta without asking again. A new human-owned choice, unclear override, missing authority or human stop suspends only the affected work.

Run the sibling Forcing Pass on the proposed state before presentation or execution. Before Persistence, recheck changes made during molding and any newly changed source or affected dependency; an unchanged reviewed delta needs no repeated full pass. Return the authorized delta and its source/permission basis to the sibling Persistence. Keep stage transitions internal unless staged review is requested.

---

### 2026-08-30 07:11 UTC

## Effective Status Markers

Status markers on changed nodes represent the effective landed status after documented tool side effects, not the pre-write status. Fixed-baseline normalization activates an edited existing `not_examined` node, so render it as `[!][~]`. A newly created leaf can remain `[+][ ]`. Use Worktree Publication to inspect actual normalization and ancestor reopening; these are not automatic semantic proof.