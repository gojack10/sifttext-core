---
id: "5f0d176d-c68a-4755-94c6-a461415a1835"
name: "Wayfinder"
micro_summary: "Tree-native multi-session planning: agree the destination, expose the sharp frontier without pretending the fog is known, resolve one decision at a time, and hand off a clear route rather than implementing it."
status: "in_progress"
see_also: [{"target":"22d73d4d-85ad-4656-8c98-3a2b0e9969e5","reason":"Use for source-owned facts that block a Wayfinder decision; Research returns evidence and does not make the decision."},{"target":"e8a42273-9959-4451-8350-f656cc6e9d3f","reason":"Use when a Wayfinder question needs a disposable observable artifact because discussion cannot settle it."},{"target":"81b3ee00-4320-4dd7-9920-34480f8431e2","reason":"Use for the human-owned decision frontier during destination and route clarification."},{"target":"991e877a-20fe-403d-96f6-75a80436a405","reason":"Use when route decisions depend on precise shared terms or conceptual boundaries."},{"target":"c72d552d-3499-445b-a8d5-05d0ff7824f2","reason":"Mandatory tree-write procedure for proposing, approving, persisting, reconciling, and verifying a Wayfinder map."}]
scope: "Plan a genuinely multi-session, decision-heavy effort whose destination can be named but whose route is still unclear. Build and refine a SiftText-native map of decision and investigation questions; stop when the route is clear, before implementation."
warnings: "Wayfinder is planning, not implementation permission; a route node must never become a disguised delivery task.\nDo not turn fog into speculative nodes or infer human decisions, assignments, claims, or approval from status.\nThe node's existence and router links do not prove automatic loading or successful fresh-agent use; verify those separately."
---
## When to use

Use Wayfinder only when the destination is meaningful, the route contains real uncertainty, and the planning work will not fit safely in one agent session. If a bounded conversation can settle the route, use the lighter relevant method and do not create a map.

## Procedure

1. Enter through Local Core Agent Entrypoint and its ordered reads in the human-facing planning role; then read the assigned project's applicable instructions, current owner, checkpoint and necessary evidence within authority. Keep a working ledger in the conversation until authorized persistence: destination, actual human decisions, evidence questions, human choices, dependencies and deferred work. Do not require an absent private project router or methodology tree.
2. Agree with the human on the whole destination, constraints and out-of-scope boundary. Preserve explicit prior decisions. Explain the current system sufficiently for the human to steer; clarify uncertainty rather than invent intent.
3. Show a compact breadth-first working ledger before diving into one branch. Separate human decisions, evidence questions, dependencies, deferred items and fog that cannot yet be framed honestly. Reason backward from the agreed destination through consequential decisions and handoffs only as far as the evidence justifies: at each edge ask what the receiving actor or system would actually do, and what must therefore already be true. The first consequential edge that cannot be justified without speculation is useful fog, not a license to invent downstream nodes. At that boundary, seek the cheapest bounded contact with reality that can discriminate the important possibilities, using Research, Prototype, direct inspection, or another fitting evidence method. Incorporate the result into the ledger and redraw only the affected route. Use Grilling and Domain Modeling when decision assumptions or shared terms need examination, reading each before applying it.
4. If the planning route has no material fog and fits a bounded conversation, explain the lighter path to the human; do not create unnecessary Wayfinder state or silently replace the requested interaction with an implementation gate. The human may mold the route.
5. Select the next answerable item. Apply and read the lightest fitting method: Grilling for human-owned decisions; Research or direct inspection for source-owned facts; Prototype for one uncertainty needing observable behavior; Domain Modeling for unsettled meaning. State what each activity will return and where the path resumes. Independent authorized evidence probes may run in parallel, but must not decide human preferences.
6. Resolve the item at its appropriate boundary. Evidence workers return sourced findings, conditions, contradictions and unknowns. The assigned substantive agent interprets them and presents any genuine human choice with practical consequences. Do not convert worker recommendations or architecture constraints into preferences the human never chose.
7. Update only the affected ledger item and its explicit dependencies after each material result or human answer. Preserve unmentioned items, reopen only invalidated decisions, show what remains and repeat from the next answerable item. Keep the complete destination visible when a branch finishes.
8. When the route supports a faithful handoff, account for every remaining planning item as answered, explicitly deferred or assigned to a named later evidence checkpoint. Turn accepted implementation work into independently testable segments with purpose, dependencies, actual inputs, observable result/check, recipient and stop conditions under the assigned project's instructions. Runtime behavior remains unproven until observed; do not require fictional certainty before a bounded implementation can be planned.
9. Review the whole route and segmented plan with the human. After explicit persistence permission, perform SiftText Commit for the accepted map, decisions and planned work. The working ledger may remain external until this final planning checkpoint; earlier persistence requires a separately approved boundary. Stop Wayfinder at the planning handoff. Do not continue into implementation without its own authority.

## Output

A resumable project map whose outline exposes the destination owner, current sharp questions, blockers, settled decisions, out-of-scope boundaries, and the next bounded planning action. Each answer carries evidence or a human decision; volatile detail stays with its canonical owner rather than being copied into the map. During the run the map may live as the external working ledger; it is not SiftText state until the human-approved final planning checkpoint lands it through SiftText Commit. Distinguish the working proposed map, the accepted plan, and landed persistence.

## Stop boundaries

Stop for a missing destination, conflicting authority, an unapproved tree delta, contradictory evidence, a human decision, or a question that cannot yet be stated without guessing. Stop Wayfinder entirely when the route is clear; implementation needs its own authorized outcome and checks. Do not claim the working ledger or route exists in SiftText before approved writes land; during planning the ledger may remain external until the final checkpoint.

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

Substantially adapted for ordinary SiftText nodes from `https://github.com/mattpocock/skills` at revision `959a8e9f1edc3adbe2f7e3054bb6fbefa6696260`, primarily `skills/engineering/wayfinder/SKILL.md` and `docs/engineering/wayfinder.md`. The destination, fog, frontier, decision-map, and planning-before-building ideas are preserved. Upstream issue trackers, issue labels, child tickets, assignee claims, setup instructions, branch conventions, context pointers, tool invocations, and slash commands are intentionally excluded; SiftText structure, links, statuses, evidence, and approval law replace them.