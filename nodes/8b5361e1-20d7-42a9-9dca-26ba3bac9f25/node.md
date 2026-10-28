---
id: "8b5361e1-20d7-42a9-9dca-26ba3bac9f25"
name: "Managed Releases and Updates"
status: "resolved"
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

Release checks run in the background every four hours.

A failed check waits at least five minutes before retrying.

A newer compatible release produces an `update_available` notice without activating it automatically.

Act on the update notice at the earliest convenient pause in your work. Do not interrupt important work to update.

Run `"$BIN" update` under the user's recorded standing permission.

When the update needs permission, ask whether the user authorizes it.

Record routine permission only after the user's approval:

```sh
"$BIN" update --grant standing
"$BIN" update --config
```

The grant command updates the `updates` object in `.sifttext/config.json` while preserving the user's other settings.

Revoke a grant with `"$BIN" update --grant ask`. Never grant permission for yourself or copy it from another machine.

On releases that lack saved update permissions, obtain permission for the current update.

## Inspect and apply

```sh
"$BIN" update --status
```

If activation was interrupted, retain its state files and run `"$BIN" update --resume`.

A coupled application/knowledge release stays pending until the candidate application can accept the knowledge update.

Continue local reads during a blocked merge.

## Reconcile a blocked knowledge update

A conflict returns a retained worktree and the affected paths.

Use [[Core Maintenance|6da0b35a-955a-4ad2-bdce-2994ba26119c]] to investigate and complete the repair within the user's permission.

Read the accepted knowledge and retained candidate before editing the updater's worktree.

Commit the correction there. Resume uses that saved correction as a new checked attempt while preserving earlier committed work.

Status changes in the committed update are submitted through the existing publication checks.

An existing project node can be associated with the repair using `"$BIN" update --attach-node=UUID` when useful.

```sh
"$BIN" update --resume
```

Read the updated files listed in `refresh_instruction_paths` before continuing the original task.

## Recover missing instruction history

An existing knowledge home may lack a record of its last merged official instructions.

Recover that record from retained releases and Git when the evidence establishes it.

The updater prepares a private draft for reconciling unavailable history.

Combine the official instructions with the user's edited copies in that draft, retaining the user's changes.

Commit the reconciled merge and resume.

Successful publication records the official revision as the starting point for later updates.

For an older updater that refuses before allocating a worktree, use a verified current release executable to perform this recovery while retaining the installed application.

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

Investigate persistent refusals through [[Core Maintenance|6da0b35a-955a-4ad2-bdce-2994ba26119c]] while preserving the journal.

Never reset accepted history or delete caches to bypass checks.

Finish by checking installed identities and pending state with "$BIN" update --status.
