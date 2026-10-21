---
id: "a472e05c-9bbf-48f2-89d7-6c6092c339ed"
name: "CLI Operations"
micro_summary: "Guides authorized use of the local Core command."
status: "in_progress"
see_also: [{"target":"6da0b35a-955a-4ad2-bdce-2994ba26119c","reason":"Use the diagnosed-problem path to matching source and maintenance guidance."}]
scope: "Operate local Core within the user's permissions."
warnings: ""
---
Read Local Core Agent Entrypoint and its task-applicable instructions before operating. Use `sifttext -h` as the command reference. Reuse unchanged instructions already read.

## Prebuilt installation

For authorized installation, follow the source repository's `INSTALL.md`. Its public command is:

```sh
curl -fsSL https://sifttext.com/install | sh
```

Open a new shell if `sifttext` is not yet on PATH. To continue in the current shell, use the default launcher at `~/.local/share/sifttext/sifttext` or the custom path selected during setup.

To upgrade a healthy managed installation, follow the child named Managed Releases and Updates because rerunning the installer leaves the existing application in place.

## Platform account

The human signs up or signs in at `https://app.sifttext.com` through Zitadel for Founding SiftText onboarding and ongoing service.

New subscribers complete a separate $50 USD/month checkout. If they already have an active $20 subscription, they keep that plan and skip checkout.

After Platform confirms paid status on its waiting page, run `sifttext login` to start PKCE browser authorization. If the browser does not open, use the authorization URL printed in the terminal.

Only server-confirmed `paid:true` produces `Authentication successful.`. Check pending or inactive payment on Platform before retrying login.

Use `sifttext account` to read server account/subscription status, refreshing authorization as needed. Account receipts contain the stable account ID, email, subscription status and server-confirmed paid flag; they omit display labels and credentials.

Use `sifttext logout` to delete local credentials even offline and attempt remote refresh-token revocation. Already-issued access tokens expire normally.

Keep native credentials under `~/.config/sifttext/account`, outside the knowledge home and Git.

Local trees stay on disk during login.

Offline knowledge commands and official release updates remain available without payment or cloud login.

The Core package supplies no hosted MCP server or cloud semantic checks.

## Select and verify the command

For an installed application, set `BIN` to its verified absolute launcher path. With a default managed installation, use `BIN="$HOME/.local/share/sifttext/sifttext"`. Ask the owner for an unknown custom path and preserve unrelated commands or configuration.

Run `"$BIN" runtime-check` to verify bundled assets. Continue with the installed command without rebuilding or cloning software.

For source development, follow the checkout's `AGENTS.md` and its required reads before building with pinned tools and checked assets.

Diagnose a failed build without changing its frozen gates.

```sh
sh scripts/setup-tools.sh
sh scripts/build.sh
sh scripts/with-tools.sh cargo build --locked --bins
CORE="$(pwd -P)"
BIN="$CORE/target/debug/sifttext"
"$BIN" -h
```

After building, an authorized source install uses `sh install-local.sh /NEW/application/directory "$HOME/sifttext"` to place the checked executable and its runtime assets together.

Choose a new application destination separate from the knowledge home.

A recognized existing home retains its edits and identity.

Install missing prerequisites within existing permission, asking before additional system changes.

## Select commit identity

Reuse a complete name/email already selected for this workspace or its intended knowledge home. Preserve existing global and repository-local Git configuration unless the owner authorizes a change.

If identity is missing, ask which name and email should appear in workspace history, offering `SiftText Agent <agent@sifttext.com>` as a default. A bounded worker returns this question to its assigning agent.

If the user offers a GitHub source, inspect it within access permission and confirm the discovered identity. Await a selection unless it was already authorized.

Follow Git Commit Skill for attribution, honoring requests for a different identity or omitted agent credit.

For a new home, choose an absolute destination with an existing physical canonical parent. Direct `init` refuses aliased or symlinked parents. Resolve an existing home with `cd -P -- "$HOME/sifttext" && pwd -P` before using commands that require its canonical path, including `advance`.

Put the selected identity in a private temporary Git config for initialization:

```sh
identity=$(mktemp)
chmod 600 "$identity"
git config --file "$identity" user.name "$name"
git config --file "$identity" user.email "$email"
GIT_CONFIG_GLOBAL="$identity" "$BIN" init "$home"
# For source installation, apply the same variable to install-local.sh instead.
rm "$identity"
```

Verify the initialized home's stored identity:

```sh
git -C "$home" config --local --get user.name
git -C "$home" config --local --get user.email
git -C "$home" log -1 --format='%an <%ae>'
```

On refusal, retain any partial home and inspect the error before retrying. Remove only the temporary config you created.

## Initialize and find instructions

Run one initialization for the selected destination. If you already initialized above, skip these examples:

```sh
"$BIN" init "$home"                              # Copy the embedded first-party KB.
# Alternatives, instead of the preceding command:
"$BIN" init "$alternate_home" --bundle "$bundle"  # Copy supplied native knowledge.
"$BIN" init "$empty_home" --empty                 # Start deliberately empty.
```

Every `init` call requires an explicit home. For the normal location, set `home="$HOME/sifttext"`. The two flags are mutually exclusive alternatives to the default seed.

The source-owned seed is compiled from readable `nodes/` and `trees/`. Initialization copies it into an independent Git repository with a checked initial commit and detached reader. Keep software and contributor checkouts outside that home.

Init checks and preserves existing recognized homes while refusing unrecognized directories and symlinks.

Preserve interrupted initialization for diagnosis before attempting repair.

A new home with the complete canonical instruction pipeline receives a generated `AGENTS.md` pointer, reported as `"pointer":"AGENTS.md"` in the init receipt. Otherwise the receipt has `"pointer":null`.

A supplied bundle's own `AGENTS.md` is excluded from the copy.

The generated pointer is untracked and excluded through `.git/info/exclude`. Rerunning init preserves it, and older homes receive no retroactive pointer.

In a clone or private draft without the pointer, `instructions` remains the supported lookup:

```sh
"$BIN" instructions "$home"
```

Read the entry file it returns and follow its task-specific instruction reads. During authorized setup, complete its Maintain Memory Continuity handoff before reporting persistent harness entry established. Preserve existing instructions; identify missing permission or an unsupported mechanism without stopping independent work. Stop dependent reads at an unreadable required file and report its exact path for repair. Return to the user's task after setup or ask what they want to work on.

Before a knowledge write, follow Worktree Publication to allocate a private draft and fixed baseline. Its checked `submit` refreshes the normal home and retrieval and removes the completed draft. Read from that home before allocating the next draft.

## Connect a project folder

For a project the user explicitly connects, add its maintained entry-node path to that folder's `AGENTS.md`:

```markdown
Read <project-entry-node-path> for this project's maintained knowledge and agent instructions. Follow its links as relevant to your task. The existing repository instructions below remain applicable.
```

Prepend the pointer while preserving the original file as an exact suffix. If the file is absent, create it with the pointer. Verify that the path resolves and only the selected project changed.

## Optional Pi session association

Before authorized Pi retention setup, read [Optional Pi session retention](../../integrations/pi/README.md). Retention is off by default and requires separate user permission for enablement or backup.

When retention is enabled and a session materially informed the knowledge, read the affected node before attaching it.

Call `sifttext_attach_session` once per relevant node using these arguments:

- Its exact UUID.
- A short kebab-case topic.
- The authorized private knowledge worktree.

Inspect the returned node and attachment name, then review the tracked association and local chat link in the draft. Continue through Worktree Publication's checks and checked submission. Keep the private transcript archive outside knowledge submissions.

If retention is disabled or a source or tool is unavailable, report the limitation and continue ordinary knowledge work.

Preserve existing Pi settings unless their modification is authorized.

## Scratch creation

For an already-approved creation, set `wt` to the allocated private worktree:

```sh
"$BIN" create "$wt" 'Scratch question' 'Keeps a reversible trial separate from accepted knowledge.'
# Use the actual returned root UUID below:
"$BIN" create "$wt" 'Trial note' 'Records evidence from the bounded local trial.' "$root_uuid"
```

After validating and applying the new structure, the helper returns the uncommitted node's UUID and path for ordinary file reads and edits.

Creation requires a private Git worktree and one cooperative writer. After partial I/O failure, preserve the draft and inspect it before retrying. Continue through Worktree Publication's full candidate checks and publication procedure.

## Failure and return

Inspect diagnostics before retrying a failed operation and repair within the assignment using installed guidance.

For unexplained behavior or a suspected defect, run `sifttext source-code` to obtain matching source and maintenance instructions.

Publishing a repair requires separate authority and privacy review.

Preserve refused drafts and unrelated work. After source-validation failure, stop dependent retrieval until repaired.

Keep accepted history and frozen limits intact during diagnosis.

Return the paths and changed UUIDs with native check receipts identifying the commit/publication boundary reached.

Support behavior claims with observed results, then return to the original task after setup or repair.
