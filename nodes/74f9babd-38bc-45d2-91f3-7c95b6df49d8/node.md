---
id: "74f9babd-38bc-45d2-91f3-7c95b6df49d8"
name: "Children"
micro_summary: "Field contract for decomposition, requiring each child's scope to sit inside the parent's and naming when a sibling or new tree is correct instead."
status: "resolved"
see_also: []
scope: "The children field. How decomposition works. When to create a child vs a sibling vs a new tree. The parent-child contract."
warnings: ""
---
Children decompose a node's scope into smaller, more specific pieces. If a node's scope is too broad to think about clearly, break it into children.

The parent-child contract: every child's scope should be a subset of the parent's scope. If a child does not belong under its parent, it is in the wrong place.

When to create a child:
- The node's scope covers multiple distinct sub-topics
- The crystallization is getting too long and covers different concerns
- You want to track status on individual pieces (one child resolved, another in progress)
- A language model would benefit from focusing on one piece at a time rather than the whole

When NOT to create a child:
- The content is short enough to live in the parent's crystallization
- You are creating a child just for organizational neatness, not because the scope is genuinely too broad
- The "child" is actually a separate concern that belongs in a different branch (make it a sibling or a node elsewhere, and link)

Children can have their own children. There is no hard depth limit, but if your tree is more than 4-5 levels deep, consider whether the decomposition has gone too far. Shallow, wide trees are often easier to navigate than deep, narrow ones.