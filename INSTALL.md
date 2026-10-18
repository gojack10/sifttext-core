# Install SiftText

SiftText gives your agent local project memory so another session can revisit a choice using the research that led to it. The agent maintains that knowledge in Markdown files with Git history, following the local instructions installed with the application.

## Confirm the installation task

Execute an installer only when the user's request authorizes installation. For an information or URL-inspection request, answer it and obtain installation permission before continuing to setup.

Use `~/.local/share/sifttext` for the application and `~/sifttext` for knowledge unless the user chose other locations. Keep both separate from project code so software replacement preserves the knowledge. If a destination conflicts with existing files, preserve them and ask how to proceed. A project can be chosen after installation.

For attended and autonomous work, act within existing authority until missing permission or an unresolved consequential choice blocks progress. Ask the human for that decision, retaining a continuation for the blocked step if they are unavailable. While awaiting it, continue independent authorized work.

For delegation, use only the harness's supported capabilities within the user's cost permission.

## Check prerequisites and identity

The release supports macOS 15+ and Linux with glibc 2.35+ on arm64 or x86_64. Before downloading it, check for Git, curl, tar and either shasum or sha256sum. The installer needs no other tool: it never requires GitHub CLI and neither downloads nor bootstraps a verifier. Handle missing prerequisites within existing permissions, obtaining approval before any additional system or privileged changes.

Reuse the Git name and email already selected for this knowledge home. If none is available, ask which identity to use, offering `SiftText Agent <agent@sifttext.com>` if the user needs a default. After they choose, use `git config --file` to put the identity in a private temporary config and set `GIT_CONFIG_GLOBAL` to that file for the installer invocation. Initialization copies the selection into the new repository's local config while leaving global Git configuration unchanged.

## Inspect release v0.2.0

The [release record](https://github.com/gojack10/sifttext-core/releases/tag/v0.2.0) provides precompiled platform packages and SHA256SUMS with source revision `6f4fd233d823daf42ef2758700233032e14d90ab`. Use that revision to inspect [install.sh](https://github.com/gojack10/sifttext-core/blob/6f4fd233d823daf42ef2758700233032e14d90ab/install.sh) under the [PolyForm Perimeter license](https://github.com/gojack10/sifttext-core/blob/6f4fd233d823daf42ef2758700233032e14d90ab/LICENSE).

Download the installer and checksum file without executing them:

```sh
review=$(mktemp -d)
base=https://github.com/gojack10/sifttext-core/releases/download/v0.2.0
curl -fsSL --proto '=https' --proto-redir '=https' "$base/install.sh" -o "$review/install.sh"
curl -fsSL --proto '=https' --proto-redir '=https' "$base/SHA256SUMS" -o "$review/SHA256SUMS"
```

Read the downloaded `install.sh` completely before running it. To compare its exact bytes with the source at the recorded revision:

```sh
source=https://raw.githubusercontent.com/gojack10/sifttext-core/6f4fd233d823daf42ef2758700233032e14d90ab
curl -fsSL --proto '=https' --proto-redir '=https' "$source/install.sh" -o "$review/source-install.sh"
cmp "$review/install.sh" "$review/source-install.sh"
```

Then verify the downloaded installer's published checksum:

```sh
(
  set -eu
  cd "$review"
  awk '$2 == "install.sh" { n++; print } END { if (n != 1) exit 1 }' SHA256SUMS > installer.sha256
  if command -v sha256sum >/dev/null; then
    sha256sum -c installer.sha256
  else
    shasum -a 256 -c installer.sha256
  fi
)
```

Stop if a download or either comparison fails, retaining the files and command output for diagnosis. A matching checksum confirms the downloaded bytes match the official release record over HTTPS; it does not establish which build produced them.

## Execute the reviewed installer

Before extracting the downloaded platform package into staging, the installer verifies its SHA-256 and checks archive paths. It then tests the staged runtime before placing the application at its destination.

For a new knowledge home, initialization copies the bundled knowledge into an independent Git repository and adds an `AGENTS.md` entrance to its local instructions. A recognized existing home keeps its contents and Git identity, so subsequent installation can reuse it without replacing authored knowledge.

If installation fails, preserve any created knowledge and use the failing command's output to determine the next authorized repair.

Managed v0.2.0 installations check official releases opportunistically after four hours and report a newer compatible release instead of applying it. The agent then runs `sifttext update` only under the standing permission the user grants. That grant is recorded with `sifttext update --grant standing`, which writes only the `updates` object of `.sifttext/config.json` inside the knowledge home and leaves other local settings intact, and `sifttext update --config` reports the current mode, grant and file path. Explain that behavior before installation so the user can choose whether to disable it. To skip the background release check, run `sifttext update --mode disabled`, or set `SIFTTEXT_NO_UPDATE=1` on the installer invocation and every subsequent SiftText invocation. On an older v0.1.0 installation none of those flags exist — it accepts only `update`, `--status`, `--resume` and `--rollback` — so ask the user before each update there instead of reading a recorded grant.

After inspection and verification succeed, execute the same downloaded file within the user's installation permission:

```sh
SIFTTEXT_RELEASE=v0.2.0 sh "$review/install.sh"
```

For custom locations, pass the application directory and knowledge-home path as the two arguments. Ensure their parent directories exist before invoking the installer so path validation can complete. The installer resolves existing parents to physical paths before setting up the home.

If you run `sifttext init` directly, its absolute destination must have an existing canonical parent; it refuses symlinked or aliased parent paths rather than writing through them. `sifttext advance` likewise requires the reader home as an absolute canonical path and refuses aliases. Use the physical path for direct commands (for an existing home, `cd -P -- "$HOME/sifttext" && pwd -P` prints it); do not work around these refusals by passing a symlink path.

## Optional private session evidence

Session retention is off by default and is a separate user choice from knowledge publication. When the user opts in, add a `session_retention` object to the existing `.sifttext/config.json` inside the knowledge home without replacing unrelated settings:

```json
{"session_retention": {"enabled": true, "archive_root": "/absolute/path/to/private/session-archive"}}
```

`archive_root` must be absolute and must already be a Git worktree; capture refuses otherwise so retained source is never left unversioned. Keep it outside the knowledge home, because the archive holds private transcripts. `sifttext session-retention-config <knowledge-home>` reports the effective settings without returning the configured remote. `sifttext session-capture`, `session-attach`, `session-backup` and `session-restore` then retain one copied source per harness session, expose it through local links under each informed node's `artifacts/chats/`, push only to a destination the user configured with `backup_enabled`, and rebuild those links on another machine. Ask before enabling retention or backup; never enable either as a side effect of installation, and never place archived source in a knowledge submission.

## Read local instructions and return to work

Use the command and knowledge-home path printed by the installer to verify the runtime and locate the local instruction files. With the defaults:

```sh
"$HOME/.local/share/sifttext/sifttext" runtime-check
"$HOME/.local/share/sifttext/sifttext" instructions "$HOME/sifttext"
```

Read the returned entry file and follow its task-specific instructions to continue the user's task. If no task is established, ask what they would like to work on. When that work produces knowledge to retain, publish it through the local worktree procedure and retrieve the saved result to verify it.

If a required instruction file is missing or unreadable, report its exact path and preserve the installation for repair. Resume at that file after an authorized repair so the remaining project work uses the installed version's guidance.

When a human installs directly, give their agent the same handoff using the actual local paths. With default locations, ask the agent to run `"$HOME/.local/share/sifttext/sifttext" instructions "$HOME/sifttext"` and read the entry file it returns; that entry names the other instruction files a task actually needs.
