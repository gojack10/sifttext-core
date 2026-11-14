---
id: "c72d552d-3499-445b-a8d5-05d0ff7824f2"
name: "SiftText Commit"
micro_summary: "Saves reviewed knowledge changes within the user's permission."
status: "resolved"
see_also: []
scope: "Carry an approved knowledge change through publication."
position: 7168
---
## Prepare usable understanding

Prepare a source-grounded proposal that leaves the affected understanding usable for the next reader. Observations made during the task may change what that reader should believe or do. Reconcile those changes in the affected knowledge, preserving historical context and making unresolved questions explicit. The user’s stated scope and approval govern what is saved.

### Recover work after compaction

If the current conversation has been compacted, use [Session Evidence](../3651b9b0-fc21-4c67-9d79-84ef3af963ce/node.md) to query its original transcript before drafting or continuing the commit. Recover every work item in the user's scope from the messages, tool calls and results. Preserve the decisions and corrections that shaped the work, evidence of what was completed, and what remains open. Record any gaps in the source and continue with the available evidence.

Keep each node focused on one work item. Update the existing node when it already owns that item.

### Example of updating affected recovery instructions

The user asks to save machine details. While drafting, the agent discovers that the same recovery node still instructs a reader to restore an archived automation. The incident is reported in [GitHub issue #36](https://github.com/gojack10/sifttext-core/issues/36).

**Bad example (A), observed excerpt**

> I only flagged that the LaunchAgents list is stale and didn't rewrite it. Refreshing that list would be a separate change.

**Better example (B), synthetic**

> While adding the machine details, I found that the recovery instructions would restore an archived job. I checked its current status and included the supported correction in the proposal.

## Permission and timing

Save when the user triggers it. A trigger is an explicit request to commit or save, or the user's yes to your suggestion to save. Draft, check and save the change without asking the user to review it first, then show what was saved.

Suggest a save at the end of an episode: the user says they are done, a task or topic finishes, or the conversation is about to switch to unrelated work. Say briefly what the save would preserve, including unfinished items and their next step. Without a trigger, keep working and do not save.

Carry an ordinary request to remember or update something to wrap-up while continuing the user's task. That request is the trigger for its own save, so it needs no second confirmation then. An explicit commit-now request brings the save forward.

Stop before saving and explain when the save needs a decision only the user can make: an incomplete or unresolved epistemic review, a waiver, or a change beyond what the user asked to preserve.

Honor a user stop or reduced scope. When the user corrects a saved change, make the correction as a follow-up save.

### Examples of reporting a triggered save

These examples are synthetic. In each, the agent suggested saving a working export method at wrap-up, and the user said yes. Start the report from what the user asked to save, say whether it was saved, and give only the detail the user needs to act on the result.

#### Asking again after the trigger

**Bad example (A)**
> Review the proposal here: https://app.sifttext.com/renders/EXAMPLE_RENDER_ID
>
> May I save this proposal?

**Better example (B)**
> Saved the export method to SiftText.
>
> Review the saved change here: https://app.sifttext.com/renders/EXAMPLE_RENDER_ID
>
> Tell me if anything should change and I'll fix it in a follow-up save.

The user already asked for the save, so a second approval request adds a turn without adding authority.

#### Stopping on an opaque failure

The save was rejected because one proposed node had no status.

**Bad example (A)**
> ```text
> error: submit rejected: missing field `status`
> ```

The agent's turn ends after this output.

**Better example (B)**
> Saving again now. The first try didn't go through because I hadn't marked whether the export method is finished or still being tested. I've marked it as still being tested, since you're trying it on other sites.

The user cannot tell from the raw error whether anything was saved, why it failed or whether the agent is still working. The better report leads with what is happening now and answers the rest in two plain sentences.

#### Onboarding the user instead of reporting

**Bad example (A)**
> Before I show you the result, here's how saving works. SiftText stores knowledge as nodes in a tree. I draft changes in a private worktree, a review checks them for claims the conversation doesn't support, and Core validates the structure before accepting them. The tree delta then marks added nodes with [+] and revised nodes with [!]. With that in mind, the export method is saved.
>
> Review the saved change here: https://app.sifttext.com/renders/EXAMPLE_RENDER_ID

**Better example (B)**
> The export method is now in SiftText, so next time we can start from what worked.
>
> Review the saved change here: https://app.sifttext.com/renders/EXAMPLE_RENDER_ID

The user asked for a save, not a tour of how saving works, and the result is buried under the tour. Lead with the result. Explain part of the process when the user asks or when it changes what they need to decide.

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

When saving understanding developed in the current conversation, use [Session Evidence](../3651b9b0-fc21-4c67-9d79-84ef3af963ce/node.md) to retain its source and associate it with the supported nodes. The retained copy keeps the original discussion recoverable after the harness clears its history.

The official GitHub/bundled user-facing KB is durable documentation, exempt from epistemic-state review. Do not run or require an epistemic check for those maintained instruction identities, bill for their review, or label them passed without a review. Review their source accuracy, instruction route and structural integrity normally. The exemption follows the identities shipped in Core's bundled KB, including locally maintained copies; it does not extend to arbitrary customer knowledge or founder development records.

For drafts that require hosted review, prepare a work packet and follow [Epistemic Check](../1b668703-b591-4790-ae6b-5c4202413de5/node.md) to review the same Git candidate that submission will use. Render the delta from the returned candidate. Repair supported findings and recheck within the user's grant. If an incomplete review or unresolved finding needs the user's decision, explain it and wait before saving.

Follow [Render a Tree Delta](../5c7e9d4c-0fe4-4f3a-a098-62177ab4d43f/node.md) and render the candidate with `sifttext tree-render` before submitting, while the private draft still exists, so the displayed change is the actual tree delta.

A proposal-only assignment ends at review: show the render and do not save.

## Save and finish

When the user has triggered the save and no decision is pending, submit the rendered candidate. When hosted review applies, use the same work packet, base, and explicit status IDs. Follow [Epistemic Check](../1b668703-b591-4790-ae6b-5c4202413de5/node.md) to handle the result and record any waiver the user explicitly approves for this save.

Use [Commit Usable Understanding](../edaefe20-50b7-4859-8645-e4c23f4e07ac/node.md) to establish what a later reader can conclude or do.

After the save succeeds, show the saved change through [Present the Reviewable Delta](../4036d3c5-e596-4a9c-b432-1c4ccc3162b4/node.md). In a chat with SiftText MCP, call `show_tree_delta` with the render ID. Always attach `Review the saved change here: [url]` when a hosted URL was returned, or the local HTML when it was not. Print a text tree or code-block delta only on explicit user request. Do not paraphrase or replace the rendered delta with your own interpretation.

Report the accepted change, the check result and any unfinished work.

Return links continue the existing procedure without restarting unchanged reads.
