---
id: "8b5361e1-20d7-42a9-9dca-26ba3bac9f25"
name: "Managed Releases and Updates"
status: "in_progress"
micro_summary: "Guides verified managed updates while preserving user knowledge."
see_also: []
---
# Managed releases and updates

Use this procedure for a binary installation made by the official installer.

Set BIN to the installed launcher's absolute path.

## Install or update

```sh
curl -fsSL https://sifttext.com/install | sh
```

INSTALL.md describes an inspect-before-run option.

Keep application files separate from the knowledge home.

```sh
"$BIN" update
```

Rerunning the installer leaves an existing application and its knowledge in place.

Local knowledge work and official release updates need no payment or cloud login.

## Release notices

Ordinary managed commands check official releases four hours after the last successful check, with a five-minute retry cooldown after failure.

Release checks run in a detached CLI process so ordinary commands do not wait for the network. Later commands display the cached result.

A newer compatible release produces an `update_available` notice without activating it automatically.

Act on the update notice at the earliest convenient pause in your work. Do not interrupt important work to update.

- With the user's recorded standing permission, run `"$BIN" update`.
- Otherwise, ask whether the user authorizes this update or routine updates.

Record routine permission only after the user's approval:

```sh
"$BIN" update --grant standing
"$BIN" update --config
```

The grant command changes only the `updates` object in the knowledge home's `.sifttext/config.json`, preserving other settings. Settings receipts show permission, file path and the pin only when set; an omitted pin means none.

Revoke a grant with `"$BIN" update --grant ask`. Never grant permission for yourself or copy it from another machine.

These settings exist in managed v0.2.0 and v0.3.0. Older v0.1.0 accepts only `update`, `--status`, `--resume` and `--rollback`, so ask before each update there.

## Inspect and apply

```sh
"$BIN" update --status
```

Preserve the journal and use checked recovery when activation is pending.

A coupled application/knowledge release stays pending until the candidate application can accept the knowledge update.

Continue local reads during a blocked merge.

## Reconcile a blocked knowledge update

A conflict returns a retained worktree and the affected paths.

Continue in an attached repair undertaking when one exists.

Otherwise use [[SiftText Commit|c72d552d-3499-445b-a8d5-05d0ff7824f2]] to create a suitable undertaking, publish it, then run:

```sh
"$BIN" update --attach-node=UUID
```

Read the accepted source and retained candidate before resolving the conflict.

Ask about unresolved semantic choices while preserving the draft.

Commit only the intended resolution in the updater's worktree.

```sh
"$BIN" update --resume
```

After activation, reread the returned refresh_instruction_paths and continue the original task.

## Pin a build

```sh
"$BIN" update --pin IDENTITY
```

While pinned, knowledge-only releases can apply with the active executable.

```sh
"$BIN" update --unpin
"$BIN" update --config
```

Treat the pin as a local user choice.

## Rollback

```sh
"$BIN" update --rollback
```

The updater reverses the exact accepted update delta while preserving later unrelated accepted changes.

For interrupted activation, inspect "$BIN" update --status and the refusal diagnostic before resuming.

Preserve a persistent refusal and its journal for maintainer-assisted repair.

Never reset accepted history or delete caches to bypass checks.

Finish by checking installed identities and pending state with "$BIN" update --status.
