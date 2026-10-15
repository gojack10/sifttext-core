---
id: "5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d"
name: "SiftText Read"
micro_summary: "Brings stored understanding into an evolving conversation without loading the entire tree."
status: "in_progress"
see_also: [{"target":"c72d552d-3499-445b-a8d5-05d0ff7824f2","reason":"Canonical procedure for tree writes within the established assignment and authority; SiftText Read adds no separate approval gate."}]
scope: "Guide source selection as a question develops, while keeping interpretation with the main reasoning agent and supporting the human's requested work, including writing and refactoring."
warnings: "Experimental procedure; storage does not prove automatic loading or successful behavior. Supports the human's assignment, including writing and refactoring; adds no separate Read approval gate."
---
## Role and entry

You are the main reasoning agent. Use this procedure to bring stored knowledge into the human's current undertaking.

Follow the human's assignment, including requested writing, reorganization, or refactoring. This reading procedure supports that work; it does not turn it into a read-only task.

The human may reshape, reorder, skip, or replace this procedure. Apply the change to the current assignment and preserve its scope. Ask only when the intended change is unclear.

Read applicable project instructions and relevant knowledge. Reuse unchanged instructions already loaded. Handle writes through SiftText Commit and the authority already established for the task; do not add a separate Read approval gate.

You own interpretation and recommendations. Decision models advise on supplied yes/no reading questions.

## 1. Establish the question

Recover the user's request, current question, goals, constraints, and any previously selected direction. Ask for clarification when ambiguity would change the investigation. Do not ask the human to remember node contents or repeat information already available.

## 2. Locate the owner

For a known area, inspect its shallow outline and checkpoint. For an unclear location, search first, then inspect the matching owner and local structure. Use supported tools and their actual schemas. Check returned coverage rather than assuming an outline is shallow or complete. Use scoped SQL metadata when necessary.

## 3. Build a reading map

Identify candidate questions, answers, sources, and relevant work. Keep large candidate lists outside the main conversation when an authorized scratchpad or controller is available. Otherwise use bounded result pages and a compact working ledger.

A candidate card contains its ID, name, micro-summary, location, and brief retrieval provenance. It is navigation, not source truth. Keep unselected candidates recoverable; do not label them irrelevant.

## 4. Read and interpret

Open the sources needed to understand the answer, its conditions, evidence, and uncertainty. Follow relevant children, links, and search leads. Do not accept a conclusion from its micro-summary.

Batch independent reads when their relevance is already clear and the interface supports it. Read sequentially when one result determines the next useful source.

Reuse a synthesis for the question and conditions it addresses. Revisit its basis when the new question, evidence, or conditions could change its meaning. A source may suggest an idea without proving that idea. Keep suggestions distinct from established facts.

## 5. Choose the next contribution

Use the current goal, evidence, dependencies, and selected direction to answer or propose a next question or action. Do not choose merely by depth, recency, or open status. Explain the material reason. Ask the human only for unresolved human-owned choices. Stop loading context when the next authorized contribution has sufficient grounding; resume discovery when a material gap appears or the question changes. This does not waive the bounded contrary-evidence search before a consequential recommendation.

## 6. Reconsider during the conversation

After a new human message or source result, check whether the question, conditions, uncertainty, or relevant relationships changed. When they did, refresh candidates and searches, including previously unselected sources. A topic can remain the same while its question changes.

Before a consequential recommendation, make a bounded wider search for qualifications, contrary evidence, or useful alternative ideas. Stay within permitted sources. Surface significant unexpected findings without silently changing the user's goal.

## 7. Optional decision support

When an authorized controller implements the child named Binary Reading Checks, read and use that contract. Without such a controller, perform selection and refresh directly; do not pretend another model ran.

A negative flag never prevents your own reading or renewed search. Repeated unhelpful selections return to your direct investigation.

## 8. Maintenance, budget, and handoff

When reading changes an existing answer or exposes competing ownership or stale structure, revisit the affected local owner and its evidence. Distinguish the operative accepted answer from historical rationale and an unproven replacement; mark disputed claims explicitly and stop dependent work until resolved. Integrate changed understanding with its existing owners rather than automatically saving another parallel answer. This is maintenance within the existing assignment and Commit process, not a separate permission gate.

Respect available context and any declared read or spending limits. Account for model continuations as well as retrieved tokens; many small nodes need not require one inference round per node. Reserve room for dialogue and continuation. Do not fill the main context with the entire candidate map or hide important omissions to make a response fit.

Before context pressure forces a handoff, save at the established Commit boundary the current question, answer and conditions, sources, unresolved questions, unfinished reading, selected direction, and next bounded step. Use existing owners. When a required write approval is pending, keep the pending checkpoint recoverable and distinguish it from landed state.

## 9. Return or continue

Return the answer or proposed next step, its decisive basis, important uncertainty, and any significant unexpected finding. Do not claim exhaustive discovery or optimal synthesis.

Continue when new input or evidence changes the question. Stop affected work for missing authority, unavailable necessary evidence, an unresolved blocking choice, or a reached resource limit. The human may change direction or stop the process.

## Flexibility and inherited knowledge

Use existing knowledge in the form available. Reading does not require the tree to be reorganized first.

Source notes, topic collections, unfinished ideas, explicit questions, partial answers, and work records are valid. Use question-and-answer structure when it exposes meaningful reasoning. Do not manufacture tasks, completion targets, or a synthesis for every collection.

For older material, distinguish recorded history from present interpretation. Recover the original question where the evidence supports it. Otherwise, state that the historical question is unknown. A question framed today may still use that material.

Evolution may revise an answer, reopen a question, create a sibling, or expose a deeper subquestion. It does not require a new node per session or an ever-deeper chain.

A requested review of an older branch is legitimate work even when no specific defect has been found. Do not restrict discovery to recently edited nodes or already-known problems. Preserve important qualifications and meaningful earlier reasoning when refactoring; historical placement is not a reason to make material unavailable.

## Worked examples

These are fictional teaching examples, not real project records or instructions to create these exact nodes. Status follows the represented claim: completing a source record does not prove all its statements. Operation markers describe the example's changes. Named cross-branch references below illustrate relationships, not real UUID links.

### 1. Earlier notes inform a question framed today

The interview records a user's experience; the correction record describes a missed changed condition. Their original conversation prompts are unavailable.

```text
[~] Product Research
|
+-- [✓] Interview Record
+-- [✓] Missed Correction Record
|
+-- [+][~] Reliable Continuation
    Add micro: Examines what a fresh agent needs to continue earlier work correctly.
    Add scope: How can prior decisions remain useful when their conditions change?
    Add cryst: Question framed during today's review.
    |
    +-- [+][~] Conditions With Conclusions
        Add micro: Develops a way to preserve the limits of reusable decisions.
        Add cryst: Working hypothesis: preserve important conditions beside
                   the conclusion. Reliable use remains unproven.
        [←] Conditions With Conclusions.see_also -> Interview Record
            Reason: Reported continuation.
        [←] Conditions With Conclusions.see_also -> Missed Correction Record
            Reason: A failure to investigate.
```

Open the hypothesis and relevant records. The older material contributes to present understanding without inventing the writer's historical intent. Reuse an existing owner when it already asks this question.

### 2. Evidence changes an existing investigation

In this fictional trial, a condition was recorded, but the reader missed its change.

```text
[~] Reliable Continuation
|
+-- [!][~] Conditions With Conclusions
|   micro unchanged: Develops a way to preserve the limits of reusable decisions.
|   Update cryst: Recording the condition did not ensure its use in this trial.
|                 The approach needs a way to prompt review.
|
+-- [+][~] Detect Changed Conditions
    Add micro: Examines when new information should reopen an earlier answer.
    Add scope: What signals should trigger a source review?
    Add cryst: The trial exposed this question. Compare the changed input
               with the earlier conditions before choosing a mechanism.
```

Inspect the new evidence, revisit the affected sources, and refine the existing answer. The larger question remains open. This need not become a new chronological synthesis layer. A later discovery could instead reopen an older branch or create a sibling.

### 3. Exploration remains valid

```text
[~] Collaboration Ideas
|   scope: Explore possible ways for people and agents to develop understanding.
|
+-- [ ] Unfinished Analogy
|   micro: Considers a research-notebook model for agent continuity.
+-- [✓] Workshop Record
|   micro: Preserves observations from one discussion.
+-- [ ] Unexpected Connection
    micro: Holds a possible relationship that needs examination.
```

When asked for new possibilities, explore the records and useful connections. Do not invent an implementation task or force the collection to resolve. When asked for an operative answer, start at its existing owner and expand as needed. Unselected material remains discoverable. A human request to reorganize changes the assignment; review that scope and carry the resulting changes through the established write process.

## Evidence boundary

This is an experimental, human-approved procedure. The examples are author walkthroughs, not fresh-agent trials. Stored instructions do not establish automatic loading, runtime integration, reliable compliance, or optimal retrieval.