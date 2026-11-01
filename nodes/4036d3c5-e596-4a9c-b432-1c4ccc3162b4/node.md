---
id: "4036d3c5-e596-4a9c-b432-1c4ccc3162b4"
name: "Present the Reviewable Delta"
micro_summary: "Shows the user exactly what would change before asking for approval."
status: "resolved"
see_also: []
scope: "Present the complete proposed knowledge change for review."
position: 2048
---
Receive the complete reviewed candidate from [[Draft From Originating Conditions|d96c84a8-ce99-4474-94ef-7ae1d1db4a1b]].

Explain each change briefly and put its exact fenced diff immediately after the explanation.

Show the complete proposed content for a new node or substantially rewritten procedure.

Use a small Unix tree when nodes are created, moved or removed.

Identify actual existing UUIDs and label temporary identities for new nodes.

Show changed parentage and exact reference targets without implying that a link moves its target.

Mark omitted ancestry explicitly.

Read [[Unix Delta Examples|198d04b9-1289-4fd1-9050-4d4bf94bfd43]] for operation notation.

Read [[Tree Development Examples|f3f63837-0754-4589-9d6b-58a2dd813741]] for structural proposals.

Group large changes into coherent reviewable parts in chat unless the user selects another review surface.

Identify any abridged rendering and keep its complete proposed payload inspectable.

Compare the outgoing presentation with the candidate before asking approval.

Ask approval for the specific changes shown unless the user explicitly preapproves them or the setting is autonomous with no user interaction.

Send approved changes to [[Save and Restore Continuation|a609e77c-8164-40d2-b391-e675194fd2ad]].

Return a substantive later change beyond that approval to the user.
