# Install SiftText

SiftText gives your agent local project memory so another session can revisit a choice using the research that led to it. The agent maintains that knowledge in Markdown files with Git history, following the local instructions installed with the application.

## Quick start

For a new installation:

```sh
curl -fsSL https://sifttext.com/install | sh
```

Open a new shell to pick up any PATH startup-file change made by the installer.

To continue in the current shell, use `"$HOME/.local/share/sifttext/sifttext"` as the default launcher.

Tell your agent to run `sifttext instructions` and read the returned entry file. For paid services, first follow the optional Platform signup and CLI login section below.

To upgrade an existing healthy managed installation, use `sifttext update` under your update permission because rerunning the installer leaves that application in place.

Agents performing setup must follow the authority and identity steps below before running either installation command.

## Confirm the installation task

Execute an installer only when the user's request authorizes installation. For an information or URL-inspection request, answer it and obtain installation permission before continuing to setup.

Keep the knowledge home separate from application files so software replacement preserves it. Unless the user chose other locations, use these defaults outside project code:

| Purpose | Default path |
| --- | --- |
| Application | `~/.local/share/sifttext` |
| Knowledge home | `~/sifttext` |

If a destination conflicts with existing files, preserve them and ask how to proceed.

For attended and autonomous work, act within existing authority until missing permission or an unresolved consequential choice blocks progress. Ask the human for that decision, retaining a continuation for the blocked step if they are unavailable. While awaiting it, continue independent authorized work.

For delegation, use only the harness's supported capabilities within the user's cost permission.

## Check prerequisites and identity

Check the [latest release](https://github.com/gojack10/sifttext-core/releases/latest) for supported platforms and operating-system requirements. Before downloading it, check for Git, curl, tar and either shasum or sha256sum. Handle missing prerequisites within existing permissions, obtaining approval before any additional system or privileged changes.

Reuse the Git name and email already selected for this knowledge home. If none is available, ask which identity to use, offering `SiftText Agent <agent@sifttext.com>` if the user needs a default. After they choose, use `git config --file` to put the identity in a private temporary config and set `GIT_CONFIG_GLOBAL` to that file for the installer invocation. Initialization copies the selection into the new repository's local config while leaving global Git configuration unchanged.

## Optional: inspect the latest release before running

The [latest official release](https://github.com/gojack10/sifttext-core/releases/latest) provides precompiled packages and SHA256SUMS. Its checksum header records the release tag and source revision. Resolve that identity once so inspection and installation use the same release even if a newer one appears.

Download the checksum file and use its recorded tag to obtain the installer without executing it:

```sh
review=$(mktemp -d)
releases=https://github.com/gojack10/sifttext-core/releases
curl -fsSL --proto '=https' --proto-redir '=https' "$releases/latest/download/SHA256SUMS" -o "$review/SHA256SUMS" || exit 1
awk 'NR == 1 {
  if (NF != 6 || $1 != "#" || $2 != "SiftText" || $3 != "release" ||
      $4 !~ /^v[0-9][a-zA-Z0-9._-]*$/ || $5 != "source" ||
      length($6) != 40 || $6 ~ /[^a-f0-9]/) exit 1
  valid = 1
} END { if (!valid) exit 1 }' "$review/SHA256SUMS" || {
  echo 'Invalid release checksum header.' >&2
  exit 1
}
read -r marker product kind release label revision < "$review/SHA256SUMS"
base="$releases/download/$release"
curl -fsSL --proto '=https' --proto-redir '=https' "$base/install.sh" -o "$review/install.sh" || exit 1
```

Read the downloaded `install.sh` completely before running it. To compare its exact bytes with the source at the recorded revision:

```sh
source="https://raw.githubusercontent.com/gojack10/sifttext-core/$revision"
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

Stop if a download or either comparison fails, retaining the files and command output for diagnosis. A matching checksum confirms agreement with the published bytes, while identifying the build that produced a package requires separate provenance evidence.

## Execute the reviewed installer

Before extracting the downloaded platform package into staging, the installer verifies its SHA-256 and checks archive paths. It then tests the staged runtime before placing the application at its destination.

For a new knowledge home, initialization copies the bundled knowledge into an independent Git repository and adds an `AGENTS.md` entrance to its local instructions. A recognized existing home keeps its contents and Git identity, so subsequent installation can reuse it without replacing authored knowledge.

If installation fails, preserve any created knowledge and use the failing command's output to determine the next authorized repair.

After inspection and verification succeed, execute the same downloaded file within the user's installation permission:

```sh
SIFTTEXT_RELEASE="$release" sh "$review/install.sh"
```

For custom locations, pass the application directory and knowledge-home path as the two arguments. Ensure their parent directories exist before invoking the installer so path validation can complete. The installer resolves existing parents to physical paths before setting up the home.

Direct `sifttext init` and `sifttext advance` commands require canonical absolute paths. For a new home, its parent must already exist. Resolve an existing home's physical path with `cd -P -- "$HOME/sifttext" && pwd -P` and use that path. Preserve a path refusal for diagnosis instead of passing a symlink alias.

## Optional Platform signup and CLI login

Choose [Plus at $20 USD/month](https://app.sifttext.com/subscribe?plan=plus) or [Pro at $50 USD/month](https://app.sifttext.com/subscribe?plan=pro), then sign in through Zitadel to continue to Stripe checkout.

Pro includes Plus's hosted MCP access and cloud sync, with these additional advertised services:

- An interactive browser for agents through MCP.
- Writing checks for AI-sounding prose.
- Automatic tree organization checks.

For an existing subscription:

| Account state | Action |
| --- | --- |
| Active | Manage the subscription through the billing portal. Do not start another subscription. |
| Payment pending | Check its status on the account page before paying again. |
| Existing $20 or $50 plan | Keep the current subscription at its existing price as Plus or Pro respectively. |

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

Local trees stay on disk during login. Where the installed help lists `sync`, its enable/status/disable operations are local opt-in controls for the native Git-backed queue; transfer requires login and retains identity/ownership guards. This source port does not newly qualify cloud service compatibility or install a background runner. Use that version's `sifttext -h` for operations.

## Updates and recovery

Managed installations can report compatible official updates without applying them automatically. Check the selected release's update policy before setup so the user can choose whether to disable background checks.

Run `sifttext update` with the user's update permission. When they authorize routine updates, record that grant with `sifttext update --grant standing`. This changes only the `updates` object of the knowledge home's `.sifttext/config.json`. Inspect the setting with `sifttext update --config`.

To skip background checks, run `sifttext update --mode disabled`. Alternatively set `SIFTTEXT_NO_UPDATE=1` during installation and every subsequent invocation. Explicit updates remain available with either setting.

For older installations, consult their installed command help and update guide before using settings flags. If standing grants are unsupported, ask before each update.

For local diagnosis, use `sifttext update --local-status` if the installed help lists it. This reads only the recorded application-update journal and activation-marker presence: no release request, cache/config/state write, lock creation/acquisition or activation. It works while ordinary readers or an updater hold the application mutex. Missing, invalid or unsafe state is refused without creating files. It does not inspect credentials, the knowledge home, Git worktrees, native save journals/tombstones or SQLite.

The receipt is **not an atomic snapshot** across state and marker, and does not verify the current launcher, accepted knowledge HEAD, pin/settings, lock ownership or publication readiness. During concurrent activation it may show last recorded identities and a marker observed at a different moment. Preserve these observations and any refusal; do not infer permission to resume, publish or remove a lock.

`update --status` retains its existing meaning: explicitly fetch/check releases and record attempt/success/cache state without installing a release. It requires the exclusive application-update mutex and can report `update busy` while an ordinary managed command holds a shared lock. `SIFTTEXT_NO_UPDATE=1` skips opportunistic checks only; it does not make `--status` read-only or disable an explicit update. Older installations without `--local-status` have no equivalent safe inspection flag: consult their guide and inspect the recorded files with ordinary read-only tools instead of treating `--status` as a lock probe.

For a blocked update, read the installed Managed Releases and Updates guide and inspect `sifttext update --local-status` (or the older version's documented status operation, with its release-check side effects understood). Reconcile any conflicting knowledge draft under that guide, inspect and commit the intended resolution, then use `sifttext update --resume` for its checked submission. Actual updates, install initialization and recovery retain exclusion against ordinary operations; local inspection never bypasses those safeguards.

To restore the retained previous release, use `sifttext update --rollback` under the installed recovery guide. Preserve any required checked knowledge reversal and follow that version's instructions to finish activation.

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

Read the returned entry file and follow its selected task-specific instructions, not a hardcoded child count. Its `version_scope:onboarding_entry_and_order` fingerprints that entry and order only; it is not a whole-home acceptance certificate. Ordinary node reads likewise do not certify the collection. During authorized setup:

1. Follow Maintain Memory Continuity to establish the selected harness's persistent entrance while preserving existing instructions.

2. Report missing permission or an unsupported mechanism as an unfinished handoff, continuing independent work.

3. Return to the user's task, or ask what they would like to work on if none is established.

4. Within knowledge-write authority, follow the local worktree procedure: edit native files, inspect and commit intended paths, then submit the commit and returned base SHA. Submit owns normalization and the exact combined deterministic gate; it reports semantic checking as deferred. Retrieve the accepted result and distinguish unfinished recovery from completion.

If a required instruction file is missing or unreadable, report its exact path and preserve the installation for repair. Resume at that file after an authorized repair so the remaining project work uses the installed version's guidance.

For an agent using a custom installation, supply the actual launcher and knowledge-home paths.
