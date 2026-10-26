---
id: "5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d"
name: "SiftText Read"
micro_summary: "Recovers relevant knowledge and makes the current interpretation understandable and actionable."
status: "in_progress"
see_also: []
scope: "Recover relevant current understanding and continue the user's authorized task with enough context to answer or act."
---
## Recover relevant understanding
Start from the user's actual question and the context already available.

Use [[Retrieve SiftText Knowledge|526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8]] to locate relevant nodes, then read their bodies and decisive evidence.

Recover the conditions and corrections that affect an earlier conclusion.

Attribute human statements to their sources.

Distinguish agent interpretations from observed results.

Retain uncertainty where the available evidence leaves the answer open.

## Explain and continue
Explain the understanding guiding the work at the user's level of familiarity.

Give the context they need to assess the result without assuming they read the nodes.

Continue the task when existing context is sufficient.

Save knowledge changes through [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]]'s approval boundary.
