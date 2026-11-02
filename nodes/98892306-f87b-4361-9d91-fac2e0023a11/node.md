---
id: "98892306-f87b-4361-9d91-fac2e0023a11"
name: "Harness Bootstrap and Migration"
micro_summary: "Installs or refreshes persistent SiftText instructions and preserves imported memory."
status: "resolved"
see_also: []
scope: "Keep the agent's persistent SiftText instructions current and first, and preserve authorized memory imports."
position: 1024
---
## Authority

Read the onboarding entry before interpreting knowledge.

Read [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]] before changing knowledge.

## Ensure harness instructions

Use the same procedure for first installation and later refreshes.

Use the entrypoint returned by `sifttext instructions` and locate Retrieve SiftText Knowledge in that home. Reuse these paths if already established.

Follow [[Keeping the Human in the Loop|6e9f23b3-3fbe-4dbd-9b58-6842438d2d01]] while handling the work. Adapt any user-facing message to whether you are installing a missing block, replacing an older one or confirming an already-current block.

### Find the persistent-instruction location

Use the mechanism documented by the current harness.

Codex uses AGENTS.md under CODEX_HOME.

Claude Code uses a user-level CLAUDE.md.

Pi uses APPEND_SYSTEM.md in its agent directory.

For another environment, inspect its available instructions, configuration and documentation. Use web research if needed and available. Resolve this yourself where possible rather than asking the user to explain their tooling.

If you still cannot establish a supported location, explain that limitation plainly and provide the prepared block for the user to place in their persistent instructions. Do not invent a settings path or claim the installation is complete.

### Make the block current and first

Fill in the Bootstrap block below with verified absolute paths.

Place one current SiftText block at the very top of the user-controlled persistent instructions, above the other content. First placement helps the agent encounter SiftText before other tooling instructions and reduces confusion with competing memory guidance. It does not change the harness's instruction hierarchy.

If you can edit that location within the user's authorization, install or replace the block yourself. Preserve unrelated content below it and remove only superseded copies of the SiftText block. If it is already current and first, leave it unchanged.

If you cannot edit the location, provide the complete block in a copyable code fence and explain where the user can put it. Politely emphasize first placement and its purpose. Adapt the request to first installation or replacement based on what you can actually see.

This is the user's maintained instruction text, not a request to inspect or disclose hidden system instructions. If this environment cannot run the CLI or read the knowledge home, explain the missing access rather than promising that pasting the block alone will make SiftText usable.

### Examples

These are synthetic examples to adapt to the actual environment.

First installation, bad example:
“Add SiftText somewhere in your settings.”

First installation, better example:
“Could you please place this block at the very top of [verified instruction location]? That helps me pick up SiftText before the other tooling instructions. Once it’s saved, we can continue with [current task].”

Replacement, bad example:
“Paste this additional block into your instructions.”

Replacement, better example:
“Could you please replace the existing SiftText block with this version and keep it at the very top of [verified instruction location]? That helps me pick up the current guidance before the other tooling instructions.”

Follow either request with the actual, fully prepared block.

### Verify and continue

Verify path resolution and the saved content where accessible. Check actual loading in a fresh conversation within permission. Distinguish a saved block from confirmed loading.

For a managed installation, after applying the block or receiving the user's confirmation, record completion with `sifttext update --harness-instructions-applied`. A request to the user is not completed installation. The acknowledgement records the agent's attestation, not Core independently verifying a harness file. It applies to this installation's active harness. When setting up another harness, follow this guide even if no refresh is pending.

Briefly report what was done and any verification still unavailable, then continue the original task.

## Bootstrap

````markdown
## SiftText

SiftText is persistent memory across chats.

Entry: <verified absolute entrypoint path>
Retrieval guide: <verified absolute path to Retrieve SiftText Knowledge>

At the start of every new chat, unless I say not to use SiftText, before any other tool call:
1. Run `sifttext instructions`.
2. Read the Entry above and follow it, plus any `agent-instructions` the command returns.
3. Before retrieving knowledge, read the Retrieval guide above, do not search first.
````

## Migrate selected memory

Confirm which sources are permitted and how the content may be shared.

Inventory the selected notes and instructions.

Preserve recoverable originals, paths and provenance.

Exclude credentials from a maintained import.

Use [[SiftText Read|5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d]] to find existing knowledge owners.

Save through [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]] and verify the maintained result.

Replace migrated memory with pointers after verification, within source-edit authority.

Preserve historical originals and unrelated instructions.

Check that a fresh conversation finds the entrypoint.

## Stop

Preserve originals and private drafts.

Report the precise continuation and continue independent authorized work.

Written instructions and a loaded harness block are separate results.
