# Install SiftText

SiftText gives your agent local project memory so another session can revisit a choice using the research that led to it. The agent maintains that knowledge in Markdown files with Git history, following the local instructions installed with the application.

## Quick start

For a new installation:

```sh
curl -fsSL https://sifttext.com/install | sh
```

Open a new shell to pick up any PATH startup-file change made by the installer.

To continue in the current shell, use `"$HOME/.local/share/sifttext/sifttext"` as the default launcher.

Follow Platform signup and CLI login below, then tell your agent to run `sifttext instructions` and read the returned entry file.

To upgrade an existing healthy managed installation, use `sifttext update` under your update permission because rerunning the installer leaves that application in place.

Agents performing setup must follow the authority and identity steps below before running either installation command.

## Confirm the installation task

Execute an installer only when the user's request authorizes installation. For an information or URL-inspection request, answer it and obtain installation permission before continuing to setup.

Use `~/.local/share/sifttext` for the application and `~/sifttext` for knowledge unless the user chose other locations. Keep both separate from project code so software replacement preserves the knowledge. If a destination conflicts with existing files, preserve them and ask how to proceed. A project can be chosen after installation.

For attended and autonomous work, act within existing authority until missing permission or an unresolved consequential choice blocks progress. Ask the human for that decision, retaining a continuation for the blocked step if they are unavailable. While awaiting it, continue independent authorized work.

For delegation, use only the harness's supported capabilities within the user's cost permission.

## Check prerequisites and identity

The release supports macOS 15+ and Linux with glibc 2.35+ on arm64 or x86_64. Before downloading it, check for Git, curl, tar and either shasum or sha256sum. Handle missing prerequisites within existing permissions, obtaining approval before any additional system or privileged changes.

Reuse the Git name and email already selected for this knowledge home. If none is available, ask which identity to use, offering `SiftText Agent <agent@sifttext.com>` if the user needs a default. After they choose, use `git config --file` to put the identity in a private temporary config and set `GIT_CONFIG_GLOBAL` to that file for the installer invocation. Initialization copies the selection into the new repository's local config while leaving global Git configuration unchanged.

## Optional: inspect release v0.3.0 before running

The [v0.3.0 release record](https://github.com/gojack10/sifttext-core/releases/tag/v0.3.0) provides precompiled platform packages and SHA256SUMS with source revision `0d926f084daceedf8d96b11d52684a0a95ed38ad`. Use that revision to inspect [install.sh](https://github.com/gojack10/sifttext-core/blob/0d926f084daceedf8d96b11d52684a0a95ed38ad/install.sh) under the [PolyForm Perimeter license](https://github.com/gojack10/sifttext-core/blob/0d926f084daceedf8d96b11d52684a0a95ed38ad/LICENSE).

Download the installer and checksum file without executing them:

```sh
review=$(mktemp -d)
base=https://github.com/gojack10/sifttext-core/releases/download/v0.3.0
curl -fsSL --proto '=https' --proto-redir '=https' "$base/install.sh" -o "$review/install.sh"
curl -fsSL --proto '=https' --proto-redir '=https' "$base/SHA256SUMS" -o "$review/SHA256SUMS"
```

Read the downloaded `install.sh` completely before running it. To compare its exact bytes with the source at the recorded revision:

```sh
source=https://raw.githubusercontent.com/gojack10/sifttext-core/0d926f084daceedf8d96b11d52684a0a95ed38ad
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

Stop if a download or either comparison fails, retaining the files and command output for diagnosis. Use a matching checksum as evidence of agreement with the official release bytes. To identify which build produced a package, obtain separate build-provenance evidence.

## Execute the reviewed installer

Before extracting the downloaded platform package into staging, the installer verifies its SHA-256 and checks archive paths. It then tests the staged runtime before placing the application at its destination.

For a new knowledge home, initialization copies the bundled knowledge into an independent Git repository and adds an `AGENTS.md` entrance to its local instructions. A recognized existing home keeps its contents and Git identity, so subsequent installation can reuse it without replacing authored knowledge.

If installation fails, preserve any created knowledge and use the failing command's output to determine the next authorized repair.

After inspection and verification succeed, execute the same downloaded file within the user's installation permission:

```sh
SIFTTEXT_RELEASE=v0.3.0 sh "$review/install.sh"
```

For custom locations, pass the application directory and knowledge-home path as the two arguments. Ensure their parent directories exist before invoking the installer so path validation can complete. The installer resolves existing parents to physical paths before setting up the home.

Direct `sifttext init` and `sifttext advance` commands require canonical absolute paths. For a new home, its parent must already exist. Resolve an existing home's physical path with `cd -P -- "$HOME/sifttext" && pwd -P` and use that path. Preserve a path refusal for diagnosis instead of passing a symlink alias.

## Platform signup and CLI login

Sign up or sign in at [app.sifttext.com](https://app.sifttext.com) through Zitadel for Founding SiftText onboarding and ongoing service.

New subscribers complete a separate $50 USD/month checkout for this service.

Existing active $20 subscribers keep their plan without another checkout.

After checkout, wait for Platform's server-confirmed paid waiting page before CLI login. A checkout redirect alone does not confirm activation.

Run:

```sh
sifttext login
```

Login uses browser authorization with PKCE and a local callback. If the browser does not open, use the authorization URL printed in the terminal.

Only server-confirmed `paid:true` produces `Authentication successful.`. If payment is pending or inactive, follow the Platform link and check its status before retrying login.

```sh
sifttext account
sifttext logout
```

Use `account` to read server-confirmed subscription status, refreshing authorization when needed.

Use `logout` to remove local credentials even offline and attempt remote refresh-token revocation.

Already-issued access tokens expire normally.

Credentials are private native files under `~/.config/sifttext/account`. Keep that directory outside the knowledge home and Git.

Offline knowledge commands and official release updates remain available without payment or cloud login.

Local trees stay on disk during login.

The Core package supplies no hosted MCP server or cloud semantic checks.

## Updates and recovery

Managed v0.2.0 and v0.3.0 installations report compatible official updates after four hours without applying them automatically. Explain this before setup so the user can choose whether to disable background checks.

Run `sifttext update` with the user's update permission. When they authorize routine updates, record that grant with `sifttext update --grant standing`. This changes only the `updates` object of the knowledge home's `.sifttext/config.json`. Inspect the setting with `sifttext update --config`.

To skip background checks, run `sifttext update --mode disabled`. Alternatively set `SIFTTEXT_NO_UPDATE=1` during installation and every subsequent invocation. Explicit updates remain available with either setting.

Older v0.1.0 accepts only `update`, `--status`, `--resume` and `--rollback`. Ask before each update there rather than trying unsupported grant flags.

For a blocked update, read the installed Managed Releases and Updates guide and inspect `sifttext update --status`. Reconcile any conflicting knowledge draft under that guide, then check and commit the resolution before `sifttext update --resume`.

To restore the retained previous release, use `sifttext update --rollback`. This also prepares any required checked knowledge reversal. The v0.3.0 recovery code can finish activation when that reversal was already accepted.

If the first resume reports native `source_freshness`, inspect the diagnostic and status before an explicit retry. Preserve a persistent refusal and its journal for repair. Never reset accepted history or delete caches to bypass it.

## Optional private session evidence

Private session retention is off by default. Enable retention or backup only when the user asks, following [the session-retention guide](integrations/pi/README.md). Keep its private Git archive outside the knowledge home and exclude transcripts from knowledge submissions.

## Read local instructions and return to work

In a new shell, verify the runtime and locate the local instruction entry:

```sh
sifttext runtime-check
sifttext instructions
```

`instructions` defaults to `~/sifttext`. For a custom home, run `sifttext instructions /absolute/knowledge-home`. If PATH is not yet refreshed, use the actual application launcher's absolute path.

Read the returned entry file and follow its task-specific instructions. During authorized setup, follow its Maintain Memory Continuity route to establish the selected harness's persistent entrance while preserving existing instructions. Report a missing permission or unsupported mechanism as an unfinished handoff and continue independent work. Then return to the user's task, or ask what they would like to work on if none is established. When that work produces knowledge to retain, publish it through the local worktree procedure and retrieve the saved result to verify it.

If a required instruction file is missing or unreadable, report its exact path and preserve the installation for repair. Resume at that file after an authorized repair so the remaining project work uses the installed version's guidance.

For an agent using a custom installation, supply the actual launcher and knowledge-home paths.
