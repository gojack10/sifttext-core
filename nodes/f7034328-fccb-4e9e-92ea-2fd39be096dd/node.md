---
id: "f7034328-fccb-4e9e-92ea-2fd39be096dd"
name: "Scope"
micro_summary: "Field reference for the boundary statement of a node, with examples of specific versus weak scopes and the decompose-when-broad rule."
status: "resolved"
see_also: []
scope: "The scope field. What this node is about. The boundary of what belongs here and what does not."
warnings: ""
---
A node's scope defines what it is about. It draws the boundary: what belongs in this node and what belongs somewhere else.

A good scope is specific enough that you know what to put here, and clear enough that someone reading it for the first time understands the node's purpose without reading the full content.

Examples of good scopes:
- "Evaluate whether to expand into the European market. Three countries shortlisted."
- "Authentication flow for the mobile app. OAuth2 with PKCE."
- "Decide which database to use for the analytics pipeline."

Examples of weak scopes:
- "Research" (too vague, what research?)
- "Important stuff" (not a boundary, everything could go here)
- "The thing we discussed on Tuesday" (not self-contained, requires context the reader may not have)

When a scope gets too broad, decompose: create children that each take a specific piece of the parent scope.