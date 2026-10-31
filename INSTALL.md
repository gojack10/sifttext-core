# Install SiftText

SiftText keeps an agent's project knowledge in local Markdown files with Git history so later sessions can revisit earlier decisions.

## Quick start

For an authorized new installation:

```sh
curl -fsSL https://sifttext.com/install | sh
```

Open a new shell to load any PATH change made by the installer. In the current shell, the default launcher is `"$HOME/.local/share/sifttext/sifttext"`.

Ask the agent to run `sifttext instructions` and follow the returned entry file. Paid services also require the optional account setup below.

Upgrade an existing managed installation with `sifttext update` under update permission, since rerunning the installer preserves its current application.

## Establish permission and destinations

For an information request, limit work to inspection until the user authorizes installation.

Use separate locations for application files and knowledge so replacing the application preserves the user's work.

| Purpose | Default path |
| --- | --- |
| Application | `~/.local/share/sifttext` |
| Knowledge home | `~/sifttext` |

Keep both locations outside project code unless the user chooses otherwise. Preserve conflicting files and ask how to handle them before changing the destination.

When a consequential choice blocks setup, ask the human while continuing independent authorized work. Retain the blocked step if they are unavailable.

Delegate only through the harness's supported capabilities and within the user's cost permission.

## Prepare the machine and Git identity

Consult the [latest release](https://github.com/gojack10/sifttext-core/releases/latest) for supported platforms and operating-system requirements.

| Required tool | Purpose |
| --- | --- |
| Git | Knowledge history. |
| curl | HTTPS downloads. |
| tar | Package extraction. |
| shasum or sha256sum | Published checksum verification. |

Install missing prerequisites within existing permission, obtaining approval for additional privileged or system changes.

Reuse the Git name and email selected for this knowledge home. If no identity is available, ask the user which to use and offer `SiftText Agent <agent@sifttext.com>` as a possible default.

Apply the chosen identity to this installation:

1. Create a private temporary Git configuration with `git config --file`.
2. Set `GIT_CONFIG_GLOBAL` to that file for the installer invocation.
3. Let initialization copy the identity into the knowledge repository's local config.

This leaves the user's global Git configuration unchanged.

## Optional installer inspection

The official release's `SHA256SUMS` header records its tag and source revision. Use those recorded values throughout inspection to stay on the same release if a newer one appears.

Download its installer without executing it:

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

Read the downloaded `install.sh` completely. Compare it with the recorded source revision:

```sh
source="https://raw.githubusercontent.com/gojack10/sifttext-core/$revision"
curl -fsSL --proto '=https' --proto-redir '=https' "$source/install.sh" -o "$review/source-install.sh"
cmp "$review/install.sh" "$review/source-install.sh"
```

Verify the installer's published checksum:

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

If a download or comparison fails, retain the files and command output for diagnosis before proceeding.

Use the checksum comparison to establish agreement with published bytes and separate provenance evidence to identify how the package was built.

## Run the reviewed installer

Within installation permission, execute the reviewed file:

```sh
SIFTTEXT_RELEASE="$release" sh "$review/install.sh"
```

The installer verifies the platform package's checksum and archive paths before extraction, then starts its staged runtime before placing the application at its destination.

| Knowledge destination | Installer behavior |
| --- | --- |
| New home | Copies bundled knowledge into an independent Git repository and adds an `AGENTS.md` entrance. |
| Recognized existing home | Preserves authored content and Git identity. |
| Failed initialization | Retains created knowledge for diagnosis and authorized repair. |

For custom locations, pass the application directory and knowledge-home path as the installer's two arguments. Create their parent directories first so the installer can resolve physical paths.

Direct `sifttext init` and `sifttext advance` commands require canonical absolute paths. A new home's parent must already exist.

Resolve an existing home with:

```sh
cd -P -- "$HOME/sifttext" && pwd -P
```

Use the returned physical path. Retain any path refusal for diagnosis instead of substituting a symlink alias.

## Optional paid services

Choose a plan and sign in through Zitadel to continue to Stripe checkout.

| Plan | Price | Included services |
| --- | --- | --- |
| [Plus](https://app.sifttext.com/subscribe?plan=plus) | $20 USD/month | Hosted MCP and knowledge sync. |
| [Pro](https://app.sifttext.com/subscribe?plan=pro) | $50 USD/month | Plus services and the additional services advertised below. |

| Additional advertised Pro service | Purpose |
| --- | --- |
| Interactive browser through MCP | Agent browser access. |
| Writing checks | Review AI-sounding prose. |
| Tree organization checks | Review knowledge structure. |

Handle an existing subscription according to its current state:

| Account state | Action |
| --- | --- |
| Active | Use the billing portal to manage the existing subscription. |
| Payment pending | Check the account page before paying again. |
| Existing $20 plan | Keep its current price as Plus. |
| Existing $50 plan | Keep its current price as Pro. |

Log in through the CLI once Platform's waiting page confirms paid activation.

```sh
sifttext login
```

Login uses browser authorization with PKCE and a local callback. If the browser stays closed, open the authorization URL printed in the terminal.

| Server-confirmed access | Login result |
| --- | --- |
| `paid:true` | Prints `Authentication successful.` |
| Pending or inactive payment | Follow the Platform link and check payment status before retrying. |

| Command | Effect |
| --- | --- |
| `sifttext account` | Reads server-confirmed account facts and returns the installed account/sync guide path. |
| `sifttext logout` | Removes local credentials, including offline, and attempts remote refresh-token revocation. |

Already-issued access tokens expire normally. Keep the private credential directory `~/.config/sifttext/account` outside the knowledge home and Git.

Offline knowledge work and official updates remain available without payment or cloud login.

### Authorize knowledge sharing

Record sharing permission and any local-only exclusions using the installed account/sync guide before enabling transfers.

| Command | Effect |
| --- | --- |
| `sync enable HOME` | Records consent and installs native user-session scheduling. |
| `sync disable HOME` | Stops further transfers and scheduling while retaining both copies. |
| `sync decline HOME` | Records the decision to decline. |
| `sync policy HOME FILE.json` | Changes selection while preserving consent. |

After an upload acknowledges durable storage of the exact Git commit, follow pending work in sync status until the server finishes processing it.

An exclusion can leave earlier uploaded history in place after the current hosted content is removed by server processing.

## Updates and recovery

Managed installations fetch release notices in a detached process so ordinary commands can use the cached result without waiting for the network.

| Check result | Next automatic check |
| --- | --- |
| Success | At least four hours later. |
| Failure | After a cooldown of at least five minutes. |

Applying an update requires the user's permission. When they grant routine update permission, record it with `sifttext update --grant standing` and inspect it with `sifttext update --config`.

Saved grants change the `updates` object in the knowledge home's `.sifttext/config.json`.

On older installations without saved grants, obtain permission for each update and follow the installed help.

### Inspect update state

| Command | Behavior |
| --- | --- |
| `update --local-status` | Reads the application-update journal and activation-marker presence without taking the application mutex or changing state. |
| `update --status` | Fetches release information and records the check under the exclusive application mutex. |

When the installed help lists `--local-status`, it can inspect state even while a reader or updater holds the application mutex.

The command refuses state it cannot safely read, without creating replacement files.

Its inspection scope is limited to the journal and marker:

| Outside its scope | Consequence |
| --- | --- |
| Credentials and knowledge files | It provides no account or knowledge-content assessment. |
| Git worktrees, save journals and SQLite | Their recovery state requires separate inspection. |
| Launcher and accepted knowledge HEAD | Recorded identities are not verification of the active installation. |
| Pin settings and lock ownership | Use the corresponding configuration or recovery procedure. |

The journal and marker are read separately, so their values can come from different moments during activation.

Preserve this output and follow the installed recovery guide before making any lock changes.

`update --status` can report `update busy` when another command holds a shared application lock.

For versions without `--local-status`, use ordinary read-only tools to inspect retained files and consult the installed guide before requesting a release check with `--status`.

### Complete a pending update

Follow the installed Managed Releases and Updates guide:

| Step | Action |
| --- | --- |
| Inspect | Read local update state using the operation supported by that version. |
| Reconcile | Investigate the refusal and repair the retained knowledge draft within the user's permission. |
| Commit | Inspect and commit the intended repair in that draft. |
| Resume | Run `sifttext update --resume` for checked submission and activation. |

A repairable refusal is not automatically a blocker. When a knowledge change needs additional permission, show its exact delta and ask for approval. Preserve the draft while waiting and continue independent work. Stop the affected operation when its prerequisite cannot be repaired within permission, such as a checksum mismatch or unavailable filesystem.

Resume submits the retained draft's committed update and any committed descendant repair through publication checks, carrying its status transitions as explicit requests.

After a manual submission from the same draft, resume retrieves the saved request and checks its accepted Git history before completing activation. Preserve the recovery files until this finishes. Do not edit `pending` or remove a save journal to force a new update.

If recovery cannot establish the original publication delta needed for rollback, preserve the journals for repair.

| Recovery state | Available operations |
| --- | --- |
| Knowledge repair pending | Ordinary commands and the original submit retry remain available. |
| Application activation interrupted | Complete activation recovery before other operations. |

The updater retains exclusive access throughout installation and recovery.

For rollback, use `sifttext update --rollback` and follow the installed guide through any required knowledge reversal and activation steps.

| Resume failure | Action |
| --- | --- |
| Native `source_freshness` | Inspect the diagnostic and status before an explicit retry. |
| Persistent refusal | Retain its journal for repair. Preserve accepted history and avoid deleting caches to bypass it. |

## Optional session evidence

Enable private session retention only when the user requests it, following [the session-retention guide](integrations/pi/README.md). Keep its archive outside the knowledge home so transcripts remain separate from submitted knowledge.

## Return to the user's task

Start the runtime and locate its instruction entry:

```sh
sifttext runtime-check
sifttext instructions
```

| Installation | Instruction command |
| --- | --- |
| Default | `sifttext instructions` uses `~/sifttext`. |
| Custom | Supply the actual launcher and absolute knowledge-home path. |

Read the returned entry and follow its task-specific links.

| Receipt field | Scope |
| --- | --- |
| `version_scope:onboarding_entry_and_order` | Identifies the entry file and child order only. |

Use Core's submission receipt to determine whether a knowledge change was accepted.

During authorized setup:

| Setup step | Action |
| --- | --- |
| Configure the harness | Follow Maintain Memory Continuity so future sessions read the entry, preserving existing instructions. |
| Handle a blocked setup | Report missing permission or unsupported harness functionality as unfinished setup. |
| Continue | Return to the user's task, or ask what they want to work on. |

For later authorized knowledge changes, follow the installed worktree procedure to commit and submit the intended paths.

| Submission phase | Result |
| --- | --- |
| Normalization | Applied to the submitted changes. |
| Deterministic checks | Applied to the combined graph. |
| Semantic review | Reported as deferred. |

Read the accepted result before reporting completion. If recovery is unfinished, retain its actual status.

When a required instruction file is unavailable, report its exact path and resume reading there after an authorized repair.
