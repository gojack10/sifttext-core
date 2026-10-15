---
id: "42b1cde8-5478-4b14-938f-c10869ffda03"
name: "Probing the Unknown"
micro_summary: "Probe pattern for unmapped problems: cheap child experiments that are explored then kept or discarded, so the tree accumulates the actual map of dead ends."
status: "in_progress"
see_also: []
scope: "How to use the tree when you don't know the shape of the problem yet. The probe pattern: create child nodes as cheap experiments, explore them, discard what fails. The tree accumulates your map of dead ends and survivors."
warnings: ""
---
Sometimes you know what you want but not how to get there. The problem space is unmapped. You can't optimize toward a goal if you don't know what the landscape looks like.

The tree gives you a way to explore systematically.

## The Probe Pattern

1. **Create a child node** for each approach you want to test. Give it a clear scope: "Can we solve this with X?"
2. **Explore it.** Research or experiment yourself, or use a separately configured agent within its authorized role. Crystallize what the evidence supports.
3. **Kill it or keep it.** If the approach fails, mark the node discarded. If it survives, keep building on it.

Each discarded node is progress. It's a path you no longer need to wonder about. The tree remembers it so you — and any agent working with you — won't waste time re-exploring it.

## Subtraction, Not Addition

The strategy is to discover where you *can't* go until the path reveals itself. You're not building up a solution from nothing. You're narrowing down from everything by eliminating what doesn't work.

This is especially powerful with language models. An LLM can propose probe candidates — it's seen many problem structures. You run the probes, crystallize findings, discard failures. The tree accumulates *your* map, calibrated to *your* constraints. Not generic advice.

## What This Looks Like in Practice

A tree exploring optimization strategies might start with 15 probe children:
- "Probe: approach A" → discarded (too slow)
- "Probe: approach B" → discarded (quality regression)
- "Probe: approach C" → survived → becomes the spine of the solution
- "Probe: approach D" → discarded (already implemented, no additional gain)

After a round of probing, the tree has a clean structural record: what was tried, what survived, and why. The next session — or the next agent — starts with a mapped landscape instead of a blank slate.

## Local Probe Work

Core stores and checks the probe structure; it does not conduct experiments or supply an assistant. A human or separately configured file-capable agent can propose approaches and execute authorized probes. Use CLI Operations for local creation and Worktree Publication for checked edits. A failed check stops dependent work; preserve the draft and repair it rather than publishing an unchecked result. Human judgment still decides whether an approach survives.