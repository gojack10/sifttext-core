---
id: "98892306-f87b-4361-9d91-fac2e0023a11"
name: "Harness Bootstrap and Migration"
micro_summary: "Sets up SiftText for the user's agent."
status: "resolved"
see_also: []
scope: "Set up the agent's persistent instructions and preserve imported memory."
---
## Authority

Read the applicable entrypoint before interpreting knowledge.

Read [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]] before a knowledge change.

## Install

Run the installed command's instructions operation and read the returned entrypoint.

Read the returned entrypoint.

Find the current harness's documented persistent-instruction location.

Codex uses AGENTS.md under CODEX_HOME.

Claude Code uses a user-level CLAUDE.md.

Pi uses APPEND_SYSTEM.md in its agent directory.

For another harness, use the mechanism its own guidance specifies.

Ask the user how to supply the instructions when that guidance is absent.

Install the bootstrap below with the verified absolute path, preserving existing content.

Verify path resolution and the actual loading behavior within permission.

Report the installed location and the checks actually performed.

## Bootstrap

````markdown
## SiftText

SiftText stores evolving memory so you and your agents can save and recover understanding across chats.

SiftText terms
- A tree groups related nodes.
- A node holds knowledge about a question or topic.
- Scope states what a node covers.
- Content records a node's current understanding.
- SiftText Read recovers relevant knowledge.
- SiftText Commit saves knowledge changes.

At the start of every chat, read and follow the onboarding node below unless you are explicitly told not to use SiftText.

```text
<verified absolute entrypoint path>
```
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

Written instructions and a loaded bootstrap are separate results.
