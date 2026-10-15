---
id: "81b3ee00-4320-4dd7-9920-34480f8431e2"
name: "Grilling"
micro_summary: "Dependency-aware decision dialogue: map the design tree, ask the answerable frontier in rounds, research facts instead of asking for them, and stop for explicit shared-understanding confirmation."
status: "in_progress"
see_also: []
scope: "Stress-test a plan, decision, or idea through a dependency-aware human interview. Ask only the current decision frontier, supply evidence and recommendations, and wait for the human to decide."
warnings: "The agent gathers facts and recommends; the human owns preferences and consequential decisions. Never answer the human's side or infer confirmation from silence.\nDo not act, persist, or implement when the frontier empties until shared understanding and the relevant permission are explicit.\nIf questions in one round depend on one another, the frontier is wrong; repair the round."
---
## When to use

Use Grilling when a human-owned plan, decision, or idea contains assumptions that should be surfaced before action. It is an interview for decisions, not a substitute for source research, prototyping, or implementation.

## Procedure

1. State the decision under examination, its current constraints, what action it would authorize, and what shared understanding must exist before the session can end.
2. Sketch the design tree privately: decisions and the decisions that depend on them. The frontier is every unresolved question whose prerequisites are already settled.
3. Ask one round containing only the current frontier. Number and title each question, explain the practical choice, and give a clear recommended answer with its reasoning. Never place a question in the same round as a prerequisite it depends on.
4. Look up environmental and source-owned facts yourself or dispatch bounded Research. Keep unaffected frontier questions moving while an independent fact is pending; wait only on branches that depend on it.
5. Treat preferences, tradeoffs, goals, and consequential choices as the human's decisions. Do not answer your own interview or infer consent from silence. Adapt to one question at a time when the human asks, without losing dependency order.
6. After each answer, update the design tree, reopen branches invalidated by new evidence, and compute the next frontier. Use precise domain language and surface contradictions rather than smoothing them over.
7. When the frontier is empty, summarize the decisions, rationale, open assumptions, and consequences in human-readable form. Ask the human to confirm the shared understanding before any action or persistence.

## Output

A compact decision record: the governing choice, settled decisions and rationale, evidence used, rejected alternatives when material, unresolved questions, and the exact action the understanding would authorize. Persist only through the applicable approval and SiftText Commit boundary.

## Stop boundaries

Stop before implementation, tree mutation, or any action that relies on the result until the human confirms shared understanding and grants the relevant permission. Stop and reframe if the human cannot explain the choice, if a needed fact is missing, or if the scope is too broad for an answerable frontier.

## Provenance and adaptation

The upstream-derived material retains this notice; it does not select a license for SiftText Core:

```text
MIT License

Copyright (c) 2026 Matt Pocock

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

Lightly adapted for SiftText from `https://github.com/mattpocock/skills` at revision `959a8e9f1edc3adbe2f7e3054bb6fbefa6696260`, using `skills/productivity/grilling/SKILL.md` and `docs/productivity/grilling.md`. The design-tree, frontier-round, recommendation, facts-versus-decisions, and confirmation mechanics are preserved. Upstream skill-tool invocation, wrappers, issue workflow, configuration-file overrides, and slash commands are intentionally excluded; human decision comprehension follows this procedure and the assigned project's applicable instructions.

---

### 2026-09-17 01:34 UTC

## Entry and return to the working ledger

Enter with a named unresolved human decision, the agreed destination/constraints, relevant evidence and decisions already settled. If a prerequisite fact is missing, return that bounded evidence question to its owner rather than asking the human to invent the fact. Explain current behavior, genuine options and practical consequences before requesting an answer. Ask the currently answerable frontier; use one question at a time when the human wants it. If the human is confused, repair understanding before seeking approval.

Return the human's actual answer, its scope and rationale, affected dependencies and remaining questions to the owning workflow's conversational ledger; persist it with the existing local owner only through SiftText Commit. Preserve untouched items; silence about an item is not cancellation or consent. A recommendation is not a decision until accepted. Recompute the frontier after each answer, and distinguish an interview round from completion of the whole planning effort. Confirm shared understanding at the decision/handoff boundary without routinely reasking settled choices. This method supports the owning workflow; it does not start a separate implementation or persistence ceremony on its own.