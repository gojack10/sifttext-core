---
id: "5e565245-14fe-406c-977c-cf94a9be246b"
name: "Local Core Agent Entrypoint"
micro_summary: "Routes the agent through required onboarding and task-specific knowledge procedures."
status: "resolved"
see_also: []
scope: "Introduce SiftText and route the agent through reading, interaction, setup and authorized knowledge changes."
position: 5120
---
Knowledge nodes live at `nodes/<id>/node.md` in the knowledge home.

Run `sifttext instructions` first. Read the returned onboarding entry and follow any `agent-instructions`. The command includes a help index. `sifttext help` lists command forms and their guides.

Every node linked below is required reading unless its link states a condition. Read conditional guides when that condition applies. Reuse unchanged guidance already read in this conversation.

## Read before continuing

Read [[Keeping the Human in the Loop|6e9f23b3-3fbe-4dbd-9b58-6842438d2d01]] for working alongside the user.

Read [[Retrieve SiftText Knowledge|526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8]] before searching for knowledge.

Read [[SiftText Read|5c2f91e4-9bd5-4b45-bbc4-e69c15c1d14d]] for interpreting recovered understanding.

Read [[Knowledge Layout|b6107909-a33b-4c63-ab0d-351ba1a1b672]] for the knowledge home's files and navigation.

## Read when needed

Before querying SiftText's knowledge index with SQL, read [[SQLite Queries|4cc0b396-4b55-415e-bb26-b51c97e6ca78]].

When you need guidance on using the SiftText CLI, read [[CLI Operations|a472e05c-9bbf-48f2-89d7-6c6092c339ed]].

When a consequential choice needs clarification, read [[Develop Understanding Through Interaction|31dc46ab-eaf1-4f1a-97ed-5ff88200c4d8]].

During authorized installation or memory import, follow [[Maintain Memory Continuity|221ca2fc-1caf-4c40-92e5-d9f55501ba1e]].

When the instructions receipt returns `harness-instructions-refresh: required`, read its `harness-instructions-guide` and follow the same procedure used for initial installation.

Before changing knowledge, read [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]] and its required procedures.

Before creating private knowledge drafts or submitting changes, read [[Worktree Publication|fb1e0c9f-8c04-4baf-91e6-4688856a14af]].

When the instructions receipt assigns reconciliation or repair, follow its actual guide path before work that depends on the affected understanding. Continue independent work within the user's permissions.

## Return

Continue the user's original task.

At completion or a context boundary, preserve meaningful new understanding through SiftText Commit within the actual grant.
