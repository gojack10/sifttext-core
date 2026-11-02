---
id: "8b5361e1-20d7-42a9-9dca-26ba3bac9f25"
name: "Managed Releases and Updates"
status: "resolved"
micro_summary: "Guides verified managed updates while preserving user knowledge."
see_also: []
position: 1024
---
# Managed releases and updates

Use this procedure for a binary installation made by the official installer.

Set BIN to the installed launcher's absolute path.

## Install or update

```sh
curl -fsSL https://sifttext.com/install | sh
```

INSTALL.md describes an inspect-before-run option.

Keep application files separate from the knowledge home so replacing the software does not replace the user's independently authored knowledge.

```sh
"$BIN" update
```

Rerunning the installer leaves an existing application and its knowledge in place.

Local knowledge work and official release updates need no payment or cloud login.

## Release notices

Ordinary commands may start a background release check when one is due. In Core's `src/updates.rs`, `opportunistic_with_notice` uses `due`: four hours since a successful check and at least five minutes since the last attempt. This is command-driven scheduling, not a timer that guarantees a check every four hours.

Ordinary commands do not prompt, wait for a release download, or apply updates. Offline work uses the local build. No update-suppression environment variable is needed.

An `update_available` notice records an available release, not proof that it can read every local format. Inspect the release's compatibility and the current pin before changing a converted home. A pin may preserve a newer private format even when an official release is advertised.

Under ask permission, follow: “Read update-guide. When the user is next present, ask for permission to update SiftText.” Keep other work moving while the user is absent.

Automatic updates are enabled by default. Under standing permission, handle the update at a convenient pause and keep the user informed through [[Keeping the Human in the Loop|6e9f23b3-3fbe-4dbd-9b58-6842438d2d01]]. The configured default is not a claim that the user explicitly granted permission. Terminal detection does not change the policy.

Inspect or change the policy:

```sh
"$BIN" update --config
"$BIN" update --grant ask
"$BIN" update --grant standing
```

The grant command preserves unrelated settings in `.sifttext/config.json`. Use `ask` when the user prefers approval before updates. Once recorded under the current policy, that choice survives future updates.

Existing installations move to automatic updates once when the new policy arrives. New installations start with the policy already recorded. Older release binaries still govern installation of the release that introduces the change. Follow the installed receipt rather than assuming an old binary already supports the new policy.

Bad example, automatic updates enabled:
“An update is available. Please tell me whether I should proceed.”

Better example:
“Looks like there’s a SiftText update. I’ll take care of it once I finish this pass.”

## Inspect and apply

```sh
"$BIN" update --status
```

If activation was interrupted, retain its state files and run `"$BIN" update --resume`.

A coupled application/knowledge release stays pending until the candidate application can accept the knowledge update.

Continue independent local work with the active build during a blocked merge. If a command reports that derived data needs preparation, follow that receipt rather than assuming reads can proceed.

The updater runs preparation with the incoming executable before activation, including retained worktrees. For a retained installation whose derived views need preparation, use `"$BIN" update --prepare` within maintenance permission. This prepares local data without selecting a new release; ordinary reads do not rebuild the whole home to cross a processing-version boundary.

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

Read the updated files listed in `refresh_instruction_paths`. Run `sifttext instructions` after the update and follow any `harness-instructions-refresh` request through its actual guide. Installing new KB nodes does not itself update the harness's persistent instructions.

Keep the user informed even when the update or instructions refresh is automatic, then continue their task. These examples are synthetic, to adapt rather than recite.

Bad example:
“Update operation successful. Application and instruction synchronization completed.”

Better example, both operations succeeded:
“The SiftText update is done, and my instructions just refreshed. Continuing with [current task] now.”

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

Treat the pin as a local user choice. Position-based homes require a position-aware writer. Before deliberately selecting an older binary, follow `docs/child-order.md` from the matching source to restore legacy order in a private draft with the newer binary, then commit and submit it. Do not reset the home or re-enable an older full-history sync runner.

## Rollback

```sh
"$BIN" update --rollback
```

The updater reverses the exact accepted update delta while preserving later unrelated accepted changes.

For interrupted activation, inspect "$BIN" update --status and the refusal diagnostic before resuming.

Investigate persistent refusals through [[Core Maintenance|6da0b35a-955a-4ad2-bdce-2994ba26119c]] while preserving the journal.

Never reset accepted history or delete caches to bypass checks.

Finish by checking installed identities and pending state with "$BIN" update --status.
