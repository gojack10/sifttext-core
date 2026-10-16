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

Managed CLI invocations check official releases after four hours since the last successful check. Failed checks use a five-minute retry cooldown. Compatible updates activate between CLI operations. There is no scheduled daemon. If the user prohibits automatic network updates, set `SIFTTEXT_NO_UPDATE=1` for their launcher invocations.

Run `"$BIN" update --status` to inspect installed identities and pending work. Run `"$BIN" update` to request a check now. The application and upstream knowledge have separate identities. A coupled release stays pending until its knowledge can be accepted with the candidate application. Local reads continue when discovery fails or a knowledge merge is blocked.

A knowledge conflict returns the retained worktree and affected paths. Read the current accepted source and the candidate there. Reconcile the conflict within the user's authority, then run `"$BIN" normalize "$home" "$draft"` and `"$BIN" check "$draft" "$home"`. Commit the checked resolution in that worktree. Run `"$BIN" update --resume` to perform checked publication and activation together. Keep this update draft under the updater's custody until it finishes.

After activation, reread the reported `refresh_instruction_paths` that govern your task. Preserve unrelated private worktrees. A home without a verified upstream baseline needs maintainer help to identify its original bundle before an automatic knowledge merge can proceed.

Use `"$BIN" update --rollback` to restore the retained previous release. A required knowledge reversal is checked in a private worktree before activation. If it conflicts with newer local work, preserve the reported draft and ask about the unresolved choice. Keep the active release available while resolving it.
