---
id: "537dcdb9-d9ac-4d97-9f8d-f50dfe3487a5"
name: "Growing Your Trees"
micro_summary: "Explains decomposition as knowledge grows, distinguishing a brief body qualification from an independently meaningful rejected approach."
status: "in_progress"
see_also: []
scope: "How local tree structure evolves from large nodes to focused children, and when a rejected approach needs its own visible owner."
warnings: ""
---
When you start using SiftText, you'll naturally put a lot on each node. Big crystallizations, brief rejection notes in the body, warnings — everything in one place. This is fine. For small problems, a single well-written node captures everything an agent needs in one hop.

As your problem grows, fat nodes stop working. A 5,000-token node dilutes the LLM's attention. Important details get buried. Updates require rewriting large blocks of text. The same information gets copy-pasted across nodes.

This is when you start decomposing.

## The Shift

Instead of one big node with everything, you break it into children — each focused on one piece. The parent becomes a summary that links to its children. Each child gets the full LLM beam without distraction.

This is the Tower of Hanoi insight from the sibling node Brain, Tree, Beam: complex problems decompose into tiny nodes, each getting full attention, and the tree holds the whole picture.

## Discarded Children vs Brief Rejection Notes

When you reject an approach, you have two options:

**Brief body note** — "Considered Redis, rejected for ops complexity." Good for one simple, self-contained qualification without an independent lifecycle. Local Core rejects an authored `ruled_out` field; use the body or a discarded child, not hosted metadata.

**Discarded child node** — a full node you explored and then marked discarded. Good when the rejection has substance: you ran experiments, collected data, discovered something worth preserving. The node keeps its own crystallization, links, and history.

As problems grow, discarded children become the better default. They're searchable, linkable, and visible in outlines. An agent scanning your tree sees them and knows not to re-explore that path — and can read *why* it failed if the details matter.

A great example: a competition tree exploring optimization strategies might have 30+ probe children. Most get discarded with full reasoning ("DEAD: +0.264 gap", "3.8x slower per step"). The tree becomes a map of everything that was tried — successes and failures alike.

## You Don't Need to Know This Upfront

A human or separately configured agent can notice when a node is hard to read or a rejected approach deserves its own child. Core does not monitor your thinking or supply those suggestions. Follow the local authoring and checked-publication instructions when changing the structure; preserve failed approaches and evidence rather than hiding them in a summary.

Start with whatever feels natural. The tree will grow with you.