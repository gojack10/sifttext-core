---
id: "045dd71e-24ae-4125-8957-20ece3d3f7a5"
name: "What is a Tree"
micro_summary: "Defines a tree as one persisted domain of nodes and gives the new-tree versus child-node test, with the root's scope as the boundary."
status: "in_progress"
see_also: []
scope: "Top-level concept. What a tree is, when to create one, what makes a good root node."
warnings: ""
---
A tree is a structured collection of nodes organized in a hierarchy. It represents one domain of thinking: a project, a problem, a decision, a body of knowledge.

The root node is the tree's identity. Its scope defines what the entire tree is about. Every other node in the tree is a piece of that root scope, decomposed into something specific enough to think about clearly.

You create a new tree when you have a new domain that does not belong inside an existing tree. A tree about your startup's product strategy is separate from a tree about your personal fitness goals. If something is a sub-topic of an existing tree, make it a child node, not a new tree.

Trees persist as local files and Git history, independently of a conversation or model. A human or separately configured agent with access to the knowledge repository can read them later.

Local Core searches the selected knowledge store across its trees, or a supplied subtree. It has no fixed tree-count ceiling, but checked resource limits still apply; an unlimited-count policy is not a capacity promise. Use Leviathan Search for the delivered lexical search path. No assistant or model connection is supplied by Core.