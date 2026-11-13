# Install SiftText

SiftText keeps an agent's project knowledge in local Markdown files with Git history so later sessions can revisit earlier decisions.

## Quick start

A user can ask their agent: “Please install sifttext.com.” That request authorizes installation and saving the running agent's persistent SiftText instructions.

For an authorized new installation:

```sh
curl -fsSL https://sifttext.com/install | sh
```

The installer opens sign-in in the user's browser and finishes account setup. Sign-in is required; the human completes browser authorization. If the installer stops with a sign-in URL, give it to the user at once; the link stays open for about ten minutes. Run the installer again once they have signed in; it continues from what is missing.

Open a new shell to load any PATH change made by the installer. In the current shell, the default launcher is `"$HOME/.local/share/sifttext/sifttext"`.

Run the installed command with `instructions`, read the returned entry file and follow Maintain Memory Continuity to save this agent's persistent SiftText instructions. The install request already authorizes that setup; do not ask again. Preserve existing instructions, report any unsupported harness step as unfinished, then return to the user's task.

For an activated trial, the installer prints `Trial active until YYYY-MM-DD.` when the expiry date is available, or `Trial active.` while it is not yet available. It also prints the notice:

> We do not sell or share your data. By using SiftText you agree to our Terms: sifttext.com/terms

If the user will not sign in, use [the source-build route](#build-from-source) instead of the installer.

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

## Prepare the machine

The installer supports macOS 15 or newer and Linux with glibc 2.35 or newer, on arm64 or x86_64. Consult the [latest release](https://github.com/gojack10/sifttext-core/releases/latest) for its platform packages.

Use the installer to acquire its prerequisites where it can. It attempts Linux package installation only as root or through passwordless sudo; it never prompts for a password. Obtain approval for any additional privileged or system changes.

| Prerequisite | What to do if setup stops |
| --- | --- |
| curl | The quick-start download itself needs curl. If missing, install it within permission before running the command. Once running, the installer can attempt Linux package installation. |
| tar | The installer attempts Linux package installation if missing. If it remains unavailable, install it within permission and rerun. |
| shasum or sha256sum | One must be available. If missing, install a SHA-256 tool within permission and rerun. |
| Git | The installer starts Apple's developer-tools installation on macOS or attempts Linux package installation. If it stops, have the user finish the macOS window or run the command the installer names, then rerun. |
| rg and fd | The installer attempts to supply missing search tools from pinned official releases. Follow the installed retrieval guide if either remains unavailable. |

The installer reuses configured Git identity where available and supplies missing name/email fields as `SiftText Agent <agent@sifttext.com>` for this home only. It leaves the user's global Git configuration unchanged; no identity question is needed.

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

The installer verifies the platform package's checksum and archive paths before extraction, starts its staged runtime, completes required sign-in and initializes the knowledge home before placing a new application at its destination. It exposes `~/.local/bin/sifttext` and, when needed, adds that directory to supported shell startup files for the next shell.

If it stops, retain the diagnostic and fix the named prerequisite before rerunning. Do not bypass a checksum, path or command-conflict refusal. A recognized managed application is reused rather than upgraded; use `sifttext update` to upgrade it. An unrelated application directory or `sifttext` command is preserved and requires a destination/conflict decision before continuing.

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

After a successful run, [complete this agent's setup](#complete-this-agents-setup) before reporting installation complete. The remaining sections are references for source builds, skills, accounts and later maintenance.

## Build from source

For someone who will not sign in, use the [GitHub source repository](https://github.com/gojack10/sifttext-core) instead of the public installer. Follow [README's source-build instructions](README.md#install-from-source), beginning with the checkout's `AGENTS.md` and local instruction pipeline. That route covers compiler/tool acquisition, the build and `install-local.sh`, without the installer's required sign-in.

Keep the source checkout, installed application and knowledge home separate. If a source-build prerequisite or permission is missing, retain the diagnostic and resolve it before continuing. Once the source-built command is installed, [complete this agent's setup](#complete-this-agents-setup) using its actual launcher and knowledge-home path.

## Commit shortcut

Initialization and managed update preparation install a missing `sifttext-commit` skill for existing Codex, Claude Code and Pi user directories. The bundled skill points to SiftText Commit in the actual knowledge home; its maintained procedure and approval boundary remain in that node.

| Harness | User skill location | Explicit invocation |
| --- | --- | --- |
| Codex | `${CODEX_HOME:-~/.codex}/skills/sifttext-commit/SKILL.md` | `$sifttext-commit` or select it from the skills menu |
| Claude Code | `${CLAUDE_CONFIG_DIR:-~/.claude}/skills/sifttext-commit/SKILL.md` | `/sifttext-commit` |
| Pi | `${PI_CODING_AGENT_DIR:-~/.pi/agent}/skills/sifttext-commit/SKILL.md` | `/skill:sifttext-commit` |

Existing skill directories are preserved, including different content or a pointer to another home. Installation does not create an unused harness's configuration directory or change its settings. A skill-installation error is reported without blocking the home or application update. To retry missing-skill installation after resolving the error or adding a harness, run `sifttext update --prepare` for a managed installation, or `sifttext init /absolute/path/to/home` for an existing home.

Start a fresh conversation to discover the skill, or use Pi's `/reload`. Other harnesses can install the [bundled pointer template](integrations/skills/sifttext-commit/SKILL.md) using their supported skill mechanism, replacing `{{COMMIT_NODE}}` with the verified absolute path to `nodes/c72d552d-3499-445b-a8d5-05d0ff7824f2/node.md` in the user's knowledge home. Skill discovery depends on the harness's own settings and support.

## Account and subscription reference

The installer runs `sifttext login` itself. Do not add a separate login or checkout step to a successful new installation. For a later interrupted or expired connection, run `sifttext login` again and let the human complete browser authorization. If the browser stays closed, give the user the authorization URL printed in the terminal.

Production login connects both the account and Workspace in one command, using browser authorization with PKCE and local callbacks. Both grants must identify the same SiftText account before they are saved. Keep the private credential directory `~/.config/sifttext/account` outside the knowledge home and Git.

| Command | Effect |
| --- | --- |
| `sifttext account` | Reads server-confirmed account facts and returns the installed account guide path. |
| `sifttext logout` | Removes local credentials, including offline, and attempts remote refresh-token revocation. Already-issued access tokens expire normally. |

For a user-requested subscription purchase, use the plan's Stripe checkout and leave payment to the human.

| Plan | Price | Included services |
| --- | --- | --- |
| [Plus](https://app.sifttext.com/subscribe?plan=plus) | $20 USD/month | Hosted MCP, knowledge sync, browser tooling and both checkers with 2,000 monthly credits. |
| [Pro](https://app.sifttext.com/subscribe?plan=pro) | $50 USD/month | Plus services, greater parallel connection capacity and 10,000 monthly credits. |

Browser use depends on the connected client's available tools.

External AI-client subscriptions are separate purchases.

Read [Epistemic Check](nodes/1b668703-b591-4790-ae6b-5c4202413de5/node.md) for hosted command usage and billing.

Use [AI Slop Writing Check](nodes/869c330b-ec65-4ecd-9d03-9a09dab87010/node.md) when writing review is requested.

Handle an existing subscription according to its current state:

| Account state | Action |
| --- | --- |
| Active | Use the billing portal to manage the existing subscription. |
| Payment pending | Check the account page before paying again. |
| Existing $20 plan | Keep its current price as Plus. |
| Existing $50 plan | Keep its current price as Pro. |

## Updates and recovery

Update prepares the managed knowledge home with the incoming executable before activating it. Older homes are converted automatically in a temporary native draft: legacy child lists become per-child positions, and UUID-targeted wiki body references become relative Markdown links. Completion is recorded in the home's `.sifttext/config.json` after publication and cleanup, preserving other settings. Existing live Git worktrees are prepared where parseable; another owner's unfinished draft is retained for reconciliation. Installation and new-worktree creation prepare their own data too. Ordinary reads refresh authored edits through Git, but never perform a processor/schema upgrade or finish an interrupted application update.

For the one-time transition from v0.3.14 or earlier, the old updater cannot run this new preparation step. After that upgrade, run `sifttext update --prepare` once before using indexed commands. This offline operation also prepares a restored or separately managed view; source builders can run `sifttext index /absolute/path/to/store` with their new executable. Except for the one-time automatic format conversion, these operations do not change authored knowledge. A missing or incompatible cache produces an explicit preparation request rather than hiding a rebuild inside a read.

An updater from v0.3.15 or earlier cannot read a knowledge package that keeps display order in per-child positions. It stops with `failure_stage:candidate_capture` and a missing `children.order` path after caching the new release under the installation's `releases/` directory. Run that cached release's executable once through the installation root, for example `SIFTTEXT_INSTALL_ROOT="$HOME/.local/share/sifttext" "$HOME/.local/share/sifttext/releases/<newest release>/sifttext" update`. It converts the home, merges the official knowledge and activates itself. Official nodes whose only local change is that conversion take the incoming text without a repair worktree. Later updates use the managed command normally.

Managed installations fetch release notices in a detached process so ordinary commands can use the cached result without waiting for the network.

| Check result | Next automatic check |
| --- | --- |
| Success | At least four hours later. |
| Failure | After a cooldown of at least five minutes. |

Automatic updates are enabled by default. Session-start `sifttext instructions` can activate an already cached available release under standing permission before reading the home. Ask permission, pins and retained repairs prevent that automatic step. Background release discovery remains command-driven; a newly discovered release can wait for the next session start or an explicit update. The agent handles any remaining update at a convenient pause and keeps the user informed. Inspect the policy with `sifttext update --config`. Use `sifttext update --grant ask` to request approval before each update, or `sifttext update --grant standing` to enable automatic updates again.

These choices are kept in the knowledge home's `.sifttext/config.json`, preserving unrelated settings. Existing installations migrate once to the automatic policy. New installations start with that migration recorded, so a later `ask` choice is preserved. Follow an older binary's installed help and permission policy when installing the release that introduces the change.

After an update, run `sifttext instructions`. If it returns `harness-instructions-refresh: required`, follow its guide to apply the current startup block and record completion with `sifttext update --harness-instructions-applied`.

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

Investigate a repairable refusal within the user's permission.

If the repair needs more permission, show its exact knowledge delta and ask for approval. Preserve the draft while waiting and continue independent work.

Stop the affected operation when its prerequisite cannot be repaired within permission. A checksum mismatch or unavailable filesystem can prevent continuation.

Resume submits the committed update from the retained draft, including committed descendant repairs. It passes that candidate's status transitions to publication as explicit requests.

After a manual submission from the same draft, resume retrieves the saved request and checks its accepted Git history before completing activation. Preserve the recovery files until this finishes. Do not edit `pending` or remove a save journal to force a new update.

If recovery cannot establish the original publication delta needed for rollback, preserve the journals for repair.

| Recovery state | Available operations |
| --- | --- |
| Knowledge repair pending, not yet accepted | Ordinary commands and the original submit retry remain available. |
| Accepted update or application preparation/activation interrupted | Run `sifttext update --resume` explicitly before other operations; reads do not perform recovery. |

The updater retains exclusive access throughout installation and recovery.

For rollback, use `sifttext update --rollback` and follow the installed guide through any required knowledge reversal and activation steps.

| Resume failure | Action |
| --- | --- |
| Native `source_freshness` | Inspect the diagnostic and status before an explicit retry. |
| Persistent refusal | Retain its journal for repair. Preserve accepted history and avoid deleting caches to bypass it. |

## Optional session evidence

Enable private session retention only when the user requests it, following [the session-retention guide](integrations/pi/README.md). Keep its archive outside the knowledge home so transcripts remain separate from submitted knowledge.

## Complete this agent's setup

After the installer finishes, use the absolute command it printed if PATH is not yet available. Locate its instruction entry:

```sh
sifttext instructions
```

| Installation | Instruction command |
| --- | --- |
| Default | `sifttext instructions` uses `SIFTTEXT_HOME` when set, otherwise `~/sifttext`. |
| Custom | Supply the actual launcher and absolute knowledge-home path. |

Read the returned entry and follow its task-specific links.

| Receipt field | Scope |
| --- | --- |
| `version_scope:onboarding_entry_and_order` | Identifies the entry file and child order only. |

Use Core's submission receipt to determine whether a knowledge change was accepted.

During authorized setup:

| Setup step | Action |
| --- | --- |
| Configure the harness | Follow Maintain Memory Continuity so future sessions read the entry, preserving existing instructions. The install request already authorizes saving this agent's persistent SiftText instructions; do not ask again. |
| Handle a blocked setup | Report missing permission or unsupported harness functionality as unfinished setup. |
| Continue | Return to the user's task, or ask what they want to work on. |

For later authorized knowledge changes, follow the installed worktree procedure to commit and submit the intended paths.

| Submission phase | Result |
| --- | --- |
| Normalization | Applied to the submitted changes. |
| Deterministic checks | Applied to the combined graph. |
| Semantic review | Official bundled documentation is exempt; non-exempt knowledge reports the actual hosted review result. |

Read the accepted result before reporting completion. If recovery is unfinished, retain its actual status.

When a required instruction file is unavailable, report its exact path and resume reading there after an authorized repair.
