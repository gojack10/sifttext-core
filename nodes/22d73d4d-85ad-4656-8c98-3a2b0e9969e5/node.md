---
id: "22d73d4d-85ad-4656-8c98-3a2b0e9969e5"
name: "Research"
micro_summary: "Source-disciplined evidence gathering: frame one answerable question, trace each material claim to primary evidence, return a cited memo, and keep decisions and unsupported inference out."
status: "resolved"
see_also: []
scope: "Answer one bounded factual question from the primary sources that own the answer, with recoverable citations, evidence conditions, and explicit uncertainty. Inform a decision without making it."
warnings: "A primary source can still be stale, ambiguous, or inapplicable; verify version, conditions, and that each citation supports its claim.\nResearch informs decisions but does not make human-owned choices or convert inference into fact.\nA delegated research worker must not recursively redelegate unless the coordinator explicitly authorizes it."
---
## When to use

Use Research when a decision or investigation waits on an external fact: documented behavior, source implementation, a specification, a first-party API, or another authoritative artifact. Do not invoke it for a human preference, a decision that must be negotiated, or a codebase question better answered by a prototype or direct inspection.

## Procedure

1. State one narrow question, the version or conditions that matter, what would count as an answer, and a stopping budget. Split broad requests before reading.
2. Identify the source that owns each part of the answer. Prefer official documentation, specifications, source code, first-party APIs, and direct measurements. Secondary material may locate evidence but does not replace it.
3. Gather only the evidence needed to answer the question. Record stable locators, dates or versions where relevant, and exact conditions that limit each claim.
4. Separate observation, source assertion, inference, contradiction, and unknown. Follow citations far enough to verify that they support the adjacent claim rather than merely discuss the topic.
5. Return a compact cited evidence memo to the owning work node or authorized artifact location. If tree persistence is requested, use SiftText Commit; do not create an unreviewed second source of truth.
6. Stop when the acceptance question is answered at the requested confidence, the evidence conflicts, primary sources are unavailable, or further reading would not change the decision. Hand the evidence back to the decision owner rather than choosing for them.

## Output

A memo containing the question, short answer, claim-by-claim primary-source citations, relevant version and conditions, contradictions or confidence limits, unresolved facts, and a recoverable source list. The owner decides what the evidence means for the project.

## Stop boundaries

Do not broaden a narrow fact question into open-ended research. Do not treat source authority as proof of current applicability. If delegation is used, give one bounded report contract and prevent recursive redelegation unless the coordinator explicitly authorizes it.

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

Moderately adapted for SiftText from `https://github.com/mattpocock/skills` at revision `959a8e9f1edc3adbe2f7e3054bb6fbefa6696260`, using `skills/engineering/research/SKILL.md` and `docs/engineering/research.md`. The primary-source and cited-findings discipline is preserved. The upstream background-agent default, repository Markdown-file requirement, issue references, research branches, context pointers, tool invocations, and slash commands are intentionally excluded; delegation and persistence follow the active SiftText project and transport rules.