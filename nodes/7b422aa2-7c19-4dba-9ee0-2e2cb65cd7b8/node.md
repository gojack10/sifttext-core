---
id: "7b422aa2-7c19-4dba-9ee0-2e2cb65cd7b8"
name: "LLM is the Hopper"
micro_summary: "The LLM navigates SiftText itself. It reads a node, sees children/links/see_also, picks the next hop. No outer orchestrator. Tree shape (links, names, descriptions) is the steering wheel."
status: "resolved"
see_also: []
scope: "How traversal happens in SiftText. The language model itself navigates the graph, reading a node and deciding what to follow next based on its current task. There is no deterministic outer script orchestrating the hops."
warnings: ""
---
In SiftText, the large language model is the graph hopper. It reads a node, sees the available children, links, see_also references, and warnings, and decides on its own what to follow next. No deterministic outer script orchestrates the traversal; the model does its own hopping of the graph.

This matters because it changes what the tree is for. The structure is not a static document the model summarizes once. It is a navigable surface the model walks through, loading context one node at a time as its current task demands. Branches that look irrelevant get skipped. Branches that look promising get expanded. The model's judgment, applied to the metadata it sees on each node, drives the path.

Two consequences follow. Node names, scopes, and link descriptions are signposts — the model only sees them at the decision point and uses them to choose where to go, so vague names lead to bad hops. The shape of the graph (what links to what, what is a child versus a see_also) is the steering wheel — the author shapes traversal by shaping connections, not by writing imperative instructions.

This is also why the Brain, Tree, Beam framing works: each hop is one beam application to one node, and the tree gives the beam somewhere to land next.