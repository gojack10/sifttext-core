# Temporary prompt: recover an interrupted Core update

Use this prompt with the repair release that supplies it. It is for an agent helping a customer whose update failed with missing Git identity, an existing unactivated knowledge package, or a release-builder path such as `/Users/runner/work/.../build/candidate.sha256`. The historical path failure is present in v0.3.5: its updater can fail before activating a downloaded newer executable. Healthy installations should use their normal update command.

## Establish the retained state

Obtain permission to recover this installation. Confirm its application directory and knowledge home; defaults are `~/.local/share/sifttext` and `~/sifttext`. Read `state.json`, any `activation.json`, `current/release.json`, and the original error with ordinary file tools. Inspect the knowledge repository's status and registered worktrees. Do not run another updater while one is active.

Keep accepted history, all drafts, the knowledge home, its existing cache, application releases, and update journals. Do not reset Git, remove a lock, erase state, delete generated files, or rerun the installer over the installation. The repaired updater no longer uses the fixed `knowledge-REVISION` and `upstream-worktree` preparation paths; leave those older attempts intact.

Reuse the customer's configured Git identity. If either field is missing, ask for that selection before setting it locally:

```sh
git -C "$home" config --get user.name
git -C "$home" config --get user.email
# Only with the customer's selected values:
git -C "$home" config --local user.name "$selected_name"
git -C "$home" config --local user.email "$selected_email"
```

## Run the repaired updater, not the old launcher

Obtain the platform package attached to the same official release as this prompt. Follow that release's `INSTALL.md` and reviewed `install.sh` download, SHA-256 and archive-path checks, but extract only into a new private directory outside the knowledge home. Do not execute the installer or replace `current`. A checksum establishes agreement with published bytes, not independent build provenance.

Set `app` to the existing canonical application directory and `repair` to the extracted package directory containing `sifttext`, `release.json`, and `build/`. Read the package's `release.json` to confirm the selected release. Then:

```sh
"$repair/sifttext" runtime-check
SIFTTEXT_INSTALL_ROOT="$app" "$repair/sifttext" update --local-status
```

If the retained state has a pending update or activation marker:

```sh
SIFTTEXT_INSTALL_ROOT="$app" "$repair/sifttext" update --resume
```

Otherwise, or after the retained older update finishes:

```sh
SIFTTEXT_INSTALL_ROOT="$app" "$repair/sifttext" update
```

These commands use the repaired executable against the existing installation journal and take the normal exclusive update lock. They do not bypass checked publication or activate software by manually changing a symlink. A pending update keeps its original target; completing it is not necessarily an update to the latest release.

## Stop conditions

- For conflicts, inspect the exact retained draft and base. Show the customer the proposed knowledge delta before changing nodes. After approval, commit only that resolution in the retained draft, then repeat the repaired executable's `update --resume`.
- If cleanup refuses missing ownership evidence, modified generated files, or unknown resources, preserve the named directory and cache. Older caches may not contain the exact-byte records required by this repair. Do not manufacture proof by rebuilding the cache or copy the report's manual deletions. Ask for a scoped recovery decision; moving specific reviewed leftovers into an external recovery directory requires separate approval and must preserve their bytes.
- Stop on an unknown baseline, incomplete application-cache receipt, incompatible journal, missing original publication receipt, dirty reader, or persistent refusal. Return the exact error and retained paths to the maintainer. Do not infer missing update state from history.
- A local pin or source patch remains customer-owned. Ask before unpinning or discarding it.

## Return the actual outcome

Read `current/release.json` and `state.json` again and report the active version and source revision, not merely the downloaded version. Run the normal launcher's `runtime-check` once after recovery. Report any remaining pending update or activation marker. Keep the extracted repair package and old attempts until the customer accepts the outcome.

This route is derived from the repaired command flow; it has not been exercised against a customer's interrupted installation.
