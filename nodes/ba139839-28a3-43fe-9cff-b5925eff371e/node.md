---
id: "ba139839-28a3-43fe-9cff-b5925eff371e"
name: "Verify"
micro_summary: "Expected landed half-done/violation loop plus the canonical Forcing Pass frontier gate: every approved field and affected outcome must support truthful continuation, not a shallow PASS."
status: "in_progress"
see_also: [{"target":"da24ff95-9d07-4e15-9571-9313d6b90f94","reason":"Canonical frontier-completeness gate applied to landed outcomes; one rule owner."}]
scope: "Phase 5 of SiftText Commit. Final half-done test on every affected node. Confirm what the tree now contains matches what the approved tree delta said it should contain."
warnings: "Do not self-certify after one shallow check. Treat the rule violation as real, not hypothetical: find and fix it, then rerun. Completion requires half-done test pass plus clean repeated adversarial passes."
---
Phase 5 of SiftText Commit. Final consistency check before declaring the commit complete. This is the second forcing pass: the first forcing pass attacks the proposed delta before Persistence; Verify attacks the landed tree after Persistence/Reconciliation.

## Core Command

**The commit violated the rules. Find and fix the violation before declaring complete.**

Your job is to find it, fix it, and then look again.

Do not say the commit is clean after one pass. If you cannot find a violation, run the search again from a different angle. Only after repeated searches fail to find any concrete remaining violation may you state that the tree appears clean.

## Half-Done Test

Run the half-done test on every node touched in Persistence and Reconciliation:

"If a fresh agent read this affected node right now, would it see the state promised by the approved tree delta?"

If the answer is no anywhere, the commit is half-done. Identify the gap, perform the missing write, then re-run the test.

Common half-done patterns:

- The approved delta had `[+][x] Probe A`, but the discarded node was never created or never discarded.
- The approved delta had `[!][~] Node X Add cryst: ...`, but Persistence skipped the append.
- The approved delta had `[!][~] Node X Update scope: ...`, but Persistence edited the wrong field.
- The approved delta had `[←][x] Add link to discarded node W`, but Persistence created a plain-text mention or unresolved uuid placeholder.
- A Reconciliation pass should have fixed a stale parent section, but the rewrite landed in the wrong section or left child-TOC prose behind.

## Landed-Violation Search Loop

After the half-done test, search the affected nodes for the rule violation that landed. This catches mistakes the pre-Persistence Forcing Pass missed or that Persistence/Reconciliation introduced.

On each pass, use this exact premise:

> "The commit violated the protocol. Find the violation and fix it."

Search specifically for:

1. **Ruled_out blobs that landed** — multiple distinct failed attempts compressed into one vitals entry instead of discarded probe nodes.
2. **DRY violations that landed** — duplicated volatile state, parent prose enumerating children, or copied state that should link to a source node.
3. **One-hop structural links / parent-child TOC prose** — direct parent/child links or parent crystallization listing children. Native child symlinks/order files and SQLite Queries already expose one-hop parentage; links are for >1-hop references, cross-tree references, or orthogonal branches. Hard rule: one-hop parent/child links do not survive. Replace them with structural language such as "read parent", "read children", or "the child named X".
4. **Wrong topology** — lateral probes placed as children, downstream probes placed as siblings, or "tried to beat leader" mistaken for downstream substrate.
5. **Plain-text references** — node names mentioned without a validated `see_also` edge when an actual graph relationship was intended; prose citations alone are not edges.
6. **Status mismatch** — a node's status marker in the approved delta does not match its actual status after Persistence.
7. **Stale connected nodes** — parent, child, backlink, or forward-link prose now contradicts the landed changes.

If a violation is found, fix it immediately, then restart the loop from the same premise: "The commit violated the protocol. Find the violation and fix it."

Repeat until repeated full searches find no concrete violation. If pass 1 finds something, keep going. If pass 2 finds something, keep going. If pass 5 finds something, keep going. The loop ends only when the search stops finding actual fixable violations.

## Final Report

Final report format: list every node touched, the writes performed on it, the violation-search passes run, any violations found/fixed, and whether the half-done test passed.

If all pass, declare the commit complete. If any fail, list the gaps explicitly and STOP — do not declare complete.

The commit is not complete until every affected node would render the approved tree delta's intended state to a fresh reader, and the landed-violation search loop has stopped finding rule violations.

---

### 2026-08-30 07:11 UTC

## Effective-Status Verification

The half-done test and status-mismatch search compare landed status with the effective status derived from the fixed-baseline native normalization and explicit requests, not a raw pre-write marker. Documented node or ancestor activation is expected landed state and passes verification. Any unexplained status difference remains a violation.

## Landed Frontier Verification

Apply the Frontier Completeness Gate in [[Forcing Pass|da24ff95-9d07-4e15-9571-9313d6b90f94]] to the landed affected outcomes. Confirm that accepted commitments have evidence or explicit unfinished or deferred owners. Apply the referenced navigation check to the actual returned outline and node content.

Report landed-field verification and fresh-agent navigation evidence separately. An author’s walkthrough does not count as a fresh-agent trial.

When a navigation or continuation repair requires a fresh-agent trial, use a separate read-only session without the repair conversation or expected answers. Give it the entry node and the user-facing questions. Record its reading path, answers, omissions and unsupported claims. Compare its results with an evidence-backed inventory prepared before the test.

A failed trial returns specific defects for repair. Do not add another rule unless the failure shows a missing or conflicting instruction. Any new semantic repair outside the approved scope requires human approval. Native checks do not enforce this dialogue, judge truth or run a fresh model trial.