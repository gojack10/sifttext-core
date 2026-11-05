---
id: "c72d552d-3499-445b-a8d5-05d0ff7824f2"
name: "SiftText Commit"
micro_summary: "Saves reviewed knowledge changes within the user's permission."
status: "resolved"
see_also: []
scope: "Carry an approved knowledge change through publication."
position: 7168
---
## Permission and timing

Ask for approval of the proposed changes before saving unless the user explicitly preapproves them or the setting is autonomous with no user interaction.

An ordinary request to remember or update something still requires approval of the proposed result.

Carry ordinary save requests to wrap-up while continuing the user's task.

An explicit commit-now request brings the proposal forward with the same approval requirement.

Honor a user stop or reduced scope.

## Required procedures
Read [Dialogue](../d00ab6fd-b475-49ef-b37a-812807879308/node.md) before drafting.

Read [Forcing Pass](../da24ff95-9d07-4e15-9571-9313d6b90f94/node.md) before review.

Read [Persistence](../af92e983-aa27-4dec-b628-2a4d0bbbd2b6/node.md) before saving.

Apply [Task-Relevant Project Rules](../314e5c0a-063d-4a81-9214-11f49747703b/node.md) when its stated conditions arise, reusing unchanged reads.

## Draft and review
Use [Draft From Originating Conditions](../d96c84a8-ce99-4474-94ef-7ae1d1db4a1b/node.md) to recover the source-grounded change.

Use [Review Origin and Response Coverage](../ab82fdf1-6a71-4e09-bffe-ed6446eadc9a/node.md) to check its meaning against the sources.

For instruction changes, apply [Canopy (Prompt Authoring Framework)](../957aa569-75be-4871-b376-6b1943cc4db4/node.md).

Use [Associate External Artifact Directories](../de7cea55-c93d-48db-a4f6-58368bf6c36f/node.md) when a node materially uses an external directory.

Use [Session Evidence](../3651b9b0-fc21-4c67-9d79-84ef3af963ce/node.md) when a session materially supports the knowledge.

The official GitHub/bundled user-facing KB is durable documentation, exempt from epistemic-state review. Do not run or require an epistemic check for those maintained instruction identities, bill for their review, or label them passed without a review. Review their source accuracy, instruction route and structural integrity normally. The exemption follows the identities shipped in Core's bundled KB, including locally maintained copies; it does not extend to arbitrary customer knowledge or founder development records.

For non-exempt proposed nodes, commit the private draft and run `sifttext epistemic-check --worktree HOME DRAFT COMMIT BASE [EXPLICIT_UUID ...]` before showing the proposal. It prepares and reviews the same immutable combined candidate as submission while retaining the draft. Use its returned candidate commit for the proposal and submission. An unchanged final tree and reviewed context reuses the authentic server result under compatible checker rules, without another model run or charge. Supplied-text and packet reviews have separate scopes and do not certify that Git candidate. Repair findings and re-check until passing. Show only a passing proposal unless something genuinely significant needs the user's decision. Neither a passing check nor documentation exemption replaces approval.

Follow [Render a Tree Delta](../5c7e9d4c-0fe4-4f3a-a098-62177ab4d43f/node.md) and render the proposal with `sifttext tree-render` so the displayed change is the actual tree delta.

Present the rendered file as made. Do not paraphrase it, re-summarize it, or replace it with your own interpretation. Limit your accompanying words to the check result and the approval request.

Show the change through [Present the Reviewable Delta](../4036d3c5-e596-4a9c-b432-1c4ccc3162b4/node.md) and obtain the required approval.

A proposal-only assignment ends at review.

## Save and finish

Before submit, read [Epistemic Check](../1b668703-b591-4790-ae6b-5c4202413de5/node.md) to handle the result of Core's binary-enforced paid submission check.

Use [Commit Usable Understanding](../edaefe20-50b7-4859-8645-e4c23f4e07ac/node.md) to establish what a later reader can conclude or do.

Report the accepted change and any unfinished work.

Return links continue the existing procedure without restarting unchanged reads.
