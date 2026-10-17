---
id: "8b5361e1-20d7-42a9-9dca-26ba3bac9f25"
name: "Managed Releases and Updates"
status: "in_progress"
micro_summary: "Installs verified prebuilt releases and reconciles official knowledge changes through checked private worktrees."
see_also: []
---
# Managed releases and updates

Use this procedure for a binary installation made by the official `install.sh`. Keep using the installed launcher as `BIN`. Source-built installations retain their existing setup procedure.

The installer is available at `https://github.com/gojack10/sifttext-core/releases/latest/download/install.sh`. Read it before execution. It checks prerequisites and the package checksum, installs into a separate application directory and initializes or recognizes the knowledge home. An existing recognized home keeps its identity and edits.

Managed CLI invocations check official releases after four hours since the last successful check. Failed checks use a five-minute retry cooldown. A check that finds a newer compatible release only reports it: the command returns its own result plus an `update_available` notice naming the candidate release, this guide and the update command. Nothing activates on its own and there is no scheduled daemon.

When an operation returns that notice, finish the operation, read this guide, and run `"$BIN" update` only when the notice reports `permission:standing`. That grant is the user's own, recorded in the `updates` object of `.sifttext/config.json` in the knowledge home. Run `"$BIN" update --config` to read the current mode, grant and file path. A notice that reports `permission:ask` means no grant is recorded: tell the user a release is available, ask once whether they want routine updates, and record their answer with `"$BIN" update --grant standing` only when they say so. That command rewrites just the `updates` object and leaves every other local setting intact, so never record a grant by editing the file yourself. Never grant it for yourself and never copy a grant from another machine's file. Run `"$BIN" update --mode disabled`, or set `SIFTTEXT_NO_UPDATE=1` for their launcher invocations, to skip the background release check itself; explicit update commands still work.

Run `"$BIN" update --status` to inspect installed identities and pending work. Run `"$BIN" update` to check and apply the latest compatible release now. The application and upstream knowledge have separate identities. A coupled release stays pending until its knowledge can be accepted with the candidate application. Local reads continue when discovery fails or a knowledge merge is blocked.

A knowledge conflict returns the retained worktree and affected paths, plus a `repair` object containing this guide's path and the customer's SiftText Maintenance tree path and ID. Read the guide at the returned path. Inspect the returned maintenance tree and any `repair_node`/`repair_node_path` already attached to this pending update. If no undertaking is attached, read relevant existing work and create or reuse one in that maintenance tree using the normal SiftText Commit procedure; publish it through checked knowledge operations. Then run `"$BIN" update --attach-node=UUID` with its UUID and retain the accepted home path returned by that command, not a temporary draft path. Later notices for this same pending update point to that stable undertaking: continue there rather than creating another undertaking or moving the pointer to a child.

Read the current accepted source and the updater's retained candidate in its worktree. Reconcile the conflict within the user's authority, then run `"$BIN" normalize "$home" "$draft"` and `"$BIN" check "$draft" "$home"`. Commit the checked resolution in that worktree. Run `"$BIN" update --resume` to perform checked publication and activation together. Keep this update draft under the updater's custody until it finishes; the published maintenance undertaking remains as the repair history after the pending transaction clears.

After activation, reread the reported `refresh_instruction_paths` that govern your task. Preserve unrelated private worktrees. A home without a verified upstream baseline needs maintainer help to identify its original bundle before an automatic knowledge merge can proceed.

Run `"$BIN" update --pin IDENTITY` to keep a chosen build through later official releases, including one the customer or their agent patched. While pinned, a release that changes only first-party knowledge still applies, because it reuses the active executable. A release that would replace the application is reported instead of staged or activated, and the notice names the pin and points at `--unpin`. Run `"$BIN" update --unpin` to let official application updates activate again, and `"$BIN" update --config` to read the current pin. The pin lives in the same user-owned settings file as the standing grant: it is the user's choice, so never pin or unpin on their behalf and never carry a pin from another machine.

Use `"$BIN" update --rollback` to restore the retained previous release. A required knowledge reversal is checked in a private worktree before activation. If it conflicts with newer local work, preserve the reported draft and ask about the unresolved choice. Keep the active release available while resolving it.
