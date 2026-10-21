---
id: "8b5361e1-20d7-42a9-9dca-26ba3bac9f25"
name: "Managed Releases and Updates"
status: "in_progress"
micro_summary: "Guides verified managed updates while preserving user knowledge."
see_also: []
---
# Managed releases and updates

Use this procedure for a binary installation made by the official installer. Set `BIN` to the installed launcher's absolute path. Source-built installations use the source setup procedure in the parent CLI Operations node.

## Install or update

For a new authorized installation, use `curl -fsSL https://sifttext.com/install | sh`. The repository's `INSTALL.md` provides an inspect-before-run option pinned to v0.3.0.

Before activation, the installer performs these checks:

- Verify official release URLs and package digests.
- Reject unsafe archive paths.
- Test the staged native runtime.

Keep application files separate from the knowledge home.

To upgrade an existing healthy managed installation, run `"$BIN" update` within the user's permission. Rerunning the installer leaves that application and its knowledge in place.

Local knowledge work and official release updates require neither payment nor cloud login.

## Release notices and permission

Ordinary managed commands check official releases four hours after the last successful check, with a five-minute retry cooldown after failure.

A newer compatible release produces an `update_available` notice without activating it automatically.

Finish the current operation before acting on its notice:

- For `permission:standing`, run `"$BIN" update` under the user's recorded grant.
- For `permission:ask`, ask whether they authorize this update or routine updates.

Record routine permission only after the user's approval:

```sh
"$BIN" update --grant standing
"$BIN" update --config
```

The grant command changes only the `updates` object in the knowledge home's `.sifttext/config.json`, preserving other settings. Settings receipts show mode, permission, file path and the pin only when set; an omitted pin means none.

Revoke a grant with `"$BIN" update --grant ask`. Never grant permission for yourself or copy it from another machine.

To skip background checks, run `"$BIN" update --mode disabled` or set `SIFTTEXT_NO_UPDATE=1` on launcher invocations. Explicit updates remain available with either setting.

These settings exist in managed v0.2.0 and v0.3.0. Older v0.1.0 accepts only `update`, `--status`, `--resume` and `--rollback`, so ask before each update there.

## Inspect and apply

Inspect the installed identities and pending work before recovery:

```sh
"$BIN" update --status
```

Status reports recorded application and knowledge identities, home, activation-pending flag and release-check result. With activation pending, those records alone do not establish the current launcher or reader state; preserve the journal and use checked recovery. Unfinished work adds its operation (update or rollback), retained worktree, resume command and any recorded commit or repair-node identity; rollback targets appear only when available. Recovery manifests, checksums and clocks remain in the installation's `state.json` and any `activation.json`, not the receipt. A failed release check does not mean the active installation or pending work disappeared.

To check and apply the latest compatible release under the user's permission, run `"$BIN" update`.

A coupled application/knowledge release stays pending until the candidate application can accept the knowledge update. During a blocked merge or discovery failure, continue local reads.

If the home has no verified upstream baseline, preserve it and ask the maintainer to identify its original bundle before automatic knowledge merging.

## Reconcile a blocked knowledge update

A conflict returns a retained worktree and affected paths. Read the guide at `repair.guide` and inspect the returned maintenance tree. If a repair undertaking is already attached through `repair_node` or `repair_node_path`, continue there.

If none is attached, use SiftText Commit to create or reuse a suitable undertaking in that maintenance tree. Publish it through checked knowledge operations, then run `"$BIN" update --attach-node=UUID`. Retain the accepted home path returned by that command. Keep the same undertaking for subsequent notices about this update.

Read the accepted source and retained candidate before resolving the conflict within the user's authority. Ask about unresolved semantic choices while preserving the draft.

Check the resolution before committing it:

```sh
"$BIN" normalize "$home" "$draft"
"$BIN" check "$draft" "$home"
```

Commit only the checked resolution in the updater's worktree. Then run `"$BIN" update --resume` for checked publication and activation. Keep the draft under updater custody until completion. The published undertaking retains the repair history after the transaction clears.

After activation, reread the task-relevant `refresh_instruction_paths` returned by the updater; an empty list is omitted. Continue the original task using those instructions while preserving unrelated worktrees.

## Keep a chosen application build

When the user chooses to retain a build, run `"$BIN" update --pin IDENTITY`.

While pinned, knowledge-only releases can apply using the active executable.

Releases that would replace the pinned application produce a notice naming the pin and `--unpin` command.

Run `"$BIN" update --unpin` only when the user authorizes official application updates again. To inspect the current pin, run `"$BIN" update --config`.

Treat the pin as a local user choice and never copy it from another machine.

## Rollback and recovery

Use `"$BIN" update --rollback` to restore the retained previous release. The updater prepares any required knowledge reversal in a private worktree. If that reversal conflicts with newer work, preserve the draft and ask about the unresolved choice.

For interrupted activation, inspect `"$BIN" update --status` and the refusal diagnostic before resuming. The v0.3.0 recovery code can finish activation after a rollback's knowledge reversal was already accepted, including when its submitted draft was removed. An older executable needs that patched code to use this recovery behavior.

If the first `"$BIN" update --resume` reports native `source_freshness`, inspect the diagnostic and pending status before an explicit retry.

Preserve a persistent refusal and its journal for maintainer-assisted repair.

Never reset accepted history or delete caches to bypass checks.

Finish by checking installed identities and pending state with `"$BIN" update --status`. Report the actual active release and any unfinished recovery before returning to the user's task.
