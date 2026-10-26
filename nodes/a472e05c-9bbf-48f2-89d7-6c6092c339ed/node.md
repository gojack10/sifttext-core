---
id: "a472e05c-9bbf-48f2-89d7-6c6092c339ed"
name: "CLI Operations"
micro_summary: "Guides authorized use of the local Core command."
status: "in_progress"
see_also: [{"target":"6da0b35a-955a-4ad2-bdce-2994ba26119c","reason":"Use the diagnosed-problem path to matching source and maintenance guidance."}]
scope: "Operate local Core within the user's permissions."
warnings: ""
---
## Install and verify

For an authorized installation, use the source repository's INSTALL.md.

```sh
curl -fsSL https://sifttext.com/install | sh
```

Open a new shell when sifttext is not yet on PATH.

Set BIN to the installed launcher, normally "$HOME/.local/share/sifttext/sifttext".

Run "$BIN" runtime-check to verify the bundled assets.

For a source build, follow the checkout's AGENTS.md and scripts.

## Initialize

Run one initialization for the selected knowledge home.

```sh
"$BIN" init "$home"
"$BIN" init "$alternate_home" --bundle "$bundle"
"$BIN" init "$empty_home" --empty
```

The default seed copies the bundled instructions into an independent Git repository.

Keep software and contributor checkouts outside the knowledge home.

Initialization preserves an existing recognized home.

A new home with the complete instruction pipeline receives an AGENTS.md pointer.

## Find instructions

```sh
"$BIN" instructions "$home"
```

Read the returned entry file and follow its applicable methods.

During authorized setup, follow [[Maintain Memory Continuity|221ca2fc-1caf-4c40-92e5-d9f55501ba1e]].

Read Worktree Publication before changing knowledge.

## Connect a project folder

Add the maintained entry-node path to the project's AGENTS.md.

```markdown
Read <project-entry-node-path> for this project's maintained knowledge. The existing repository instructions below remain applicable.
```

Preserve the original file as an exact suffix and verify that the path resolves.

## Session evidence

Use [[Session Evidence|3651b9b0-fc21-4c67-9d79-84ef3af963ce]] during Commit when a session materially supports the knowledge.

## Configuration

Read the existing settings and the installed CLI's supported options before changing configuration.

Preserve unrelated settings and do not write undocumented fields.

## Failure

Inspect a failed operation before retrying it.

Run "$BIN" source-code to obtain matching source and maintenance guidance.

Review privacy before publishing a repair.
