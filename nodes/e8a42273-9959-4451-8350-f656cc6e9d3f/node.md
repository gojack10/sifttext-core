---
id: "e8a42273-9959-4451-8350-f656cc6e9d3f"
name: "Prototype"
micro_summary: "Question-first prototyping: choose one uncertainty, expose a small real behavior a human or bounded test can inspect, record the verdict, and stop before production hardening."
status: "in_progress"
see_also: []
scope: "Build the smallest disposable, inspectable artifact that can answer one unresolved design, state, or interaction question. Preserve the observed answer and limits, not prototype code as production truth."
warnings: "A prototype proves only the named question under the observed scenario; a mockup, launch receipt, or polished artifact is not production or integration evidence.\nKeep disposable code isolated and never move it into production without separate implementation, safety, and validation work.\nDo not prototype a whole product or let the agent choose a human-owned preference."
---
## When to use

Use Prototype when discussion cannot settle one concrete question about behavior, state, flow, or presentation. Do not use it when the design is already settled, when the task is implementation, or when diagnosing an existing defect requires evidence from the real system.

## Procedure

1. Write the single question at the top of the work: what uncertainty will this artifact resolve, who will inspect it, and what observation would answer it?
2. Specify the observation checkpoint before building: real input or scenario, visible behavior or artifact, judge, success signal, limits and explicit stop. Use the assigned project's applicable inspection requirements; this bundle does not require an absent external dialogue method.
3. Choose the cheapest form that makes the uncertainty observable. For state or logic, expose transitions and relevant state through guided scenarios and free exploration. For presentation, compare structurally different alternatives in representative context. Use a simpler artifact when it answers the question.
4. Mark the artifact as disposable and keep it isolated from production behavior. Prefer in-memory or scratch state, existing project conventions, and only the handling needed to make the run safe and inspectable. Avoid abstractions, generalized infrastructure, speculative persistence, and polish unrelated to the question.
5. Put the artifact in front of the named judge or bounded test. Capture what entered, what happened, what was observed, and where the prototype stopped. A saved file or successful launch is not the verdict.
6. Record the answer, evidence locator, rejected assumptions, and remaining uncertainty with the owning project question. Keep exact artifact details in their authoritative location; use SiftText for the durable result and evidence boundary.
7. Stop after the question is answered or the prototype proves unable to answer it. Production implementation, hardening, migration, and broader validation are separate work.

## Output

One runnable or directly inspectable artifact, a one-sentence question, a reproducible scenario, the observed verdict, evidence locator, limitations, and the next decision. If no useful observation can be exposed, return that limitation instead of expanding the prototype.

## Stop boundaries

Do not prototype an entire product, choose on the human's behalf, merge disposable code into production, or present a mockup as runtime proof. Contradictory evidence, unavailable inspection, scope growth, or production dependencies stop the run and return it to the owning question.

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

Moderately adapted for SiftText from `https://github.com/mattpocock/skills` at revision `959a8e9f1edc3adbe2f7e3054bb6fbefa6696260`, using `skills/engineering/prototype/SKILL.md`, `skills/engineering/prototype/LOGIC.md`, `skills/engineering/prototype/UI.md`, and `docs/engineering/prototype.md`. The one-question, throwaway, observable-state discipline is preserved. Upstream branch names, commits, implementation issues, context pointers, repository placement rules, tool invocations, and slash commands are intentionally excluded; artifact custody and checks follow the current project.