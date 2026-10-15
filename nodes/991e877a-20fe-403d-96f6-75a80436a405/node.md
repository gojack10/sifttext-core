---
id: "991e877a-20fe-403d-96f6-75a80436a405"
name: "Domain Modeling"
micro_summary: "Active domain-language discipline: challenge ambiguous terms, test boundaries with scenarios and implementation evidence, record concise canonical meanings, and preserve decisions separately."
status: "in_progress"
see_also: [{"target":"c72d552d-3499-445b-a8d5-05d0ff7824f2","reason":"Mandatory procedure for persisting reviewed vocabulary or decision changes into SiftText."}]
scope: "Sharpen a project's shared domain language and conceptual boundaries while decisions are made. Resolve overloaded terms through scenarios and evidence, then refresh the canonical SiftText reference without turning it into a specification."
warnings: "Reuse the canonical reference owner; do not create a competing glossary or append exceptions beneath contradicted guidance.\nDefinitions hold domain meaning, not implementation details, transient plans, or a running specification.\nAn unreviewed agent-authored vocabulary is not authority, and an accepted term does not prove implementation matches it."
---
## When to use

Use Domain Modeling when people, code, or project records use conflicting, vague, or overloaded language, or when a consequential design choice depends on the domain boundary. Merely reading an established vocabulary does not require this skill.

## Procedure

1. Read the current local canonical reference, owning project question, applicable project instructions and relevant implementation or evidence. Identify which term or relationship is actually unsettled. Distinguish accepted meaning, historical rationale, disputed claims and an unproven replacement; do not require an absent external reference-refresh law.
2. Call out conflicts immediately. Name the existing meaning and the competing meaning; do not silently pick one or let one word stand for several concepts.
3. Propose the smallest precise canonical term, then test it with concrete normal, edge, and failure scenarios. Ask what the concept is, what it is not, where its boundary lies, and which former synonyms should be avoided.
4. Compare the proposed model with actual code, data, behavior, and accepted decisions. Surface contradictions as questions. Do not rewrite the evidence to fit the preferred vocabulary.
5. Once the human settles the meaning, prepare a concise canonical definition with examples, boundaries, and rejected synonyms. Keep implementation details, transient plans, and general specifications out of the glossary/reference.
6. Record a separate durable decision only when the choice is hard to reverse, surprising without context, and the result of a real tradeoff. Otherwise the canonical term is sufficient.
7. Refresh the existing reference owner rather than creating a duplicate glossary. Use SiftText Commit for any tree write, replace obsolete guidance after acceptance, and keep accepted design distinct from verified implementation.

## Output

A reviewed canonical term or relationship definition, avoided synonyms, boundary examples, evidence conflicts and their resolution, plus a separate decision record only when the three-part durability bar is met. The result names its authoritative owner and what remains unverified.

## Stop boundaries

Stop for unresolved human meaning, contradictory evidence, an unclear reference owner, or an unapproved write. Do not manufacture domain language for the human, encode implementation detail as a definition, or let an agent-authored glossary become authority without review.

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

Moderately adapted for SiftText from `https://github.com/mattpocock/skills` at revision `959a8e9f1edc3adbe2f7e3054bb6fbefa6696260`, using `skills/engineering/domain-modeling/SKILL.md`, `skills/engineering/domain-modeling/CONTEXT-FORMAT.md`, `skills/engineering/domain-modeling/ADR-FORMAT.md`, and `docs/engineering/domain-modeling.md`. The active vocabulary, scenario, code-cross-check, concise glossary, and three-part durable-decision bar are preserved. Upstream `CONTEXT.md`, `CONTEXT-MAP.md`, ADR directories, repository file layout, issue references, tool invocations, and slash commands are intentionally excluded; canonical references and decisions use their existing SiftText owners.