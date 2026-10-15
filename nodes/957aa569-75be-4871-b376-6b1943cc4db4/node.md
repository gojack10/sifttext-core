---
id: "957aa569-75be-4871-b376-6b1943cc4db4"
name: "Canopy (Prompt Authoring Framework)"
micro_summary: "Jack's posture for writing and repairing any agent prompt: state the contract (roles, read order, outputs, stops) explicitly enough for the weakest model meant to obey it, and repair every failure at the broadest justified generality instead of patching the incident. Not case-law notes on individual prompts."
status: "in_progress"
see_also: []
scope: "Author or revise any agent-facing instruction artifact — a chat prompt, system prompt, skill, or a procedure spread across SiftText nodes. Aliases: prompt improvement, prompt repair, instruction and skill/pipeline authoring, general versus incident-specific correction. Make the default role, reading and execution pipeline explicit and human-moldable, so readers do not invent missing transitions. Applies to any agent prompt or packaged skill, wherever it is stored, and not to ordinary knowledge or evidence nodes. SiftText Commit review is one consumer of this standard, not its owner."
warnings: "No evals behind this. It is Jack's unquantified posture, merged from his problem-solving and agentic-LLM-use epistemics — enforced as a working standard, not as a measured effect. Do not cite it as benchmark evidence. Saved instructions are never behavioral proof: fresh-agent verification is separate evidence and is not recorded here."
---
## Applicability

Apply when creating or changing any agent-facing instruction artifact — a chat or system prompt, a packaged skill, or a node that tells agents how to perform a workflow, including one spread across nodes. Ordinary knowledge, evidence and historical records do not need an invented workflow. This node owns the authoring standard; applicable SiftText Commit review makes compliance required, not optional because it is stored in Skill Library.

## Authoring procedure

1. Identify the workflow's existing owner, intended outcome, entrypoint and actors. Reuse ownership; do not create a competing procedure. Preserve original human wording where it establishes intent or authority; distinguish the author's interpretation, existing method and proposed change. Attribute conditions and limits to the actor who selected them.
2. Define role entry from the actual assignment and interaction. Distinguish the human-facing agent, coordinator, substantive reasoning agent and bounded workers where relevant. Name who receives each result and who may make each decision. If the role cannot be determined without inventing intent, specify the clarification step.
3. Specify required reads in execution order. Mark conditional reads with their trigger and distinguish reference-only links. State when already-loaded unchanged instructions may be reused. A link or method name alone is not evidence of a required read.
4. Write the default execution sequence. For each meaningful step, state its input, responsible actor, required output/evidence, permitted action and condition for advancing. The sequence may reference another canonical method; it must not paraphrase that method into a competing procedure.
5. Specify branches: what triggers research, human dialogue, a prototype, revision, continuation or a stop; what each returns; and where execution resumes. Do not leave agents to infer transitions from a collection of related nodes. Do not pre-invent future project answers or demand one node per command.
6. Make the route visible to the human at the level needed to steer. State the current step, remaining work and unresolved choices. The human may reshape, reorder, skip or replace workflow steps. Clarify an ambiguous override, preserve its scope and consequences, and update the remaining route; do not silently reinterpret it as a permanent global change. Permissions, truthful evidence claims and applicable safety boundaries remain explicit.
7. Define handoff and completion precisely. Preserve the human's assignment and requested interaction; distinguish a worker/turn returning, a branch finishing, a plan being accepted, persistence landing and implementation succeeding. Name what remains unproven and what requires separate authority.
8. Walk the proposed instructions from entry to exit, including at least one conditional branch and one human override where applicable. Include a branch where revising an agent-selected method is appropriate and one where an explicit human constraint remains binding. Repair missing reads, contradictory roles, unspecified returns, duplicate procedure owners and false completion paths before presenting or persisting. A repair after a failure runs this step and also the checks under Generalizable prompt repair; neither replaces the other. Follow SiftText Commit for writes. Saved instructions are not behavioral proof; record fresh-agent verification separately.

## Generalizable prompt repair

When repairing instructions after a failure, distinguish the observed incident, its supported causes and the reusable correction. Model, provider, project or implementation characteristics may genuinely contribute; preserve that evidence rather than assuming those details are irrelevant. In the proposal, list the supported causes and name the cause each change addresses. When the cause is uncertain, propose a discriminating test instead of claiming a preferred repair is proved. Drop a change that addresses no listed cause.

Seek the broadest justified correction by default. Removing or narrowing an obsolete instruction can be that correction; adding or broadening a rule is not automatically better. Express it through explicit roles, responsibilities, inputs, outputs, constraints, transitions and stop conditions—not merely an exception for the model or task that exposed the problem. Keep necessary deployment-specific conditions explicit and separate from the general rule.

Do not rely on a stronger model to infer missing instructions or compensate for ambiguity. Make the contract explicit enough for the least-capable model intended to perform that role, while remaining clear to stronger models. A stronger model's failure raises the urgency of investigating and repairing the instruction; it does not alone establish the cause or universality of the failure.

Before presenting the repair, walk it through the triggering case and a materially different applicable case, including a different model or capability level. Check whether the same omission can recur, whether unrelated legitimate work remains permitted, and whether success requires unstated knowledge. Repair those gaps.

These author walkthroughs inspect the proposed contract, not observed agent behavior; text fitted only to the triggering case can pass a narrow walkthrough. So also test each proposed clause against the incident's surface details. Imagine the same cause showing up with a different request, wording, application or model. Rewrite any clause that would no longer apply so it states the cause, or remove it. This test varies the incident, not the destination: a condition the destination genuinely owns stays, kept separate as above.

Unless the human explicitly requests a specialized fix, prefer a generalizable correction over an incident-specific patch. Keep walkthrough results distinct from actual behavioral tests; state what remains unverified. Maintain an existing canonical owner rather than a second authoring procedure.

## Output and stop

The output is one coherent, executable default procedure with its canonical references, explicit read/transition order, human override path and verification boundary. Stop the affected authoring action if ownership, authority or a transition cannot be stated without guessing. Preserve unrelated work. Do not fill a procedural gap by inventing the human's project decisions.

---

### 2026-09-30 22:14 UTC

## Show the actual instruction changes

Before asking the human to approve an instruction change, show the exact proposed text and its destination node and field. Show each change to existing text as a fenced `diff` block: neighboring unchanged lines as context, removed text as `-` lines and added text as `+` lines. Put each sentence of changed prose on its own diff line so every swap shows where it happens; this split is for review only and does not change the stored line breaks. For a whole-section rewrite, you may instead name the section and show its complete replacement.

A summary of the intended improvement, a node name, or “apply Canopy” is not sufficient. Distinguish unchanged context, new instructions, replacements, and deletions.

For procedures spread across nodes, expose the changed entry, assignments, dependencies, return paths, and stop conditions together. Split large proposals into readable review units without hiding consequential changes.

Follow the canonical SiftText Commit approval boundary. Showing text does not authorize persistence or execution. Use its existing Dialogue rules to render actual link changes; presentation requirements do not justify adding unnecessary links.

### Fictional example: adding an instruction

The following examples illustrate presentation only. They are not instructions to create these example nodes. Placeholder identifiers and fictional names must not be turned into real graph links.

Destination: Validate Daily Export → scope.
Operation: add this instruction to the empty scope.

> Read the supplied CSV. Report duplicate invoice IDs with row numbers. Do not modify the file. Finish when every reported duplicate can be reproduced from the named rows, or report that the input is unavailable.

### Fictional example: replacing an ambiguous instruction

Destination: Validate Daily Export → crystallization.
Operation: replace the second sentence.

```diff
 Read the export manifest.
-Check the export and continue when it looks correct.
+Run the declared CSV checks.
+Continue only if they pass.
+If a check fails, return its output and stop dependent work.
+A missing input is not a passed check.
 Report the row count.
```

### Fictional example: showing a dependency link

    [←][~] Validate Daily Export
    micro unchanged: Checks exported records before downstream delivery.
    Add scope link → Export Format
    Relationship: required input contract, not an optional reference.

Exact proposed sentence, with an illustrative placeholder only:

    Before checking the CSV, read [[Export Format|<resolved-node-id>]].

A real target must be resolved before persistence. Plain text saying “Export Format” is not a stored link. Do not add a redundant direct parent/child link or invent an extra dependency merely to demonstrate this notation.