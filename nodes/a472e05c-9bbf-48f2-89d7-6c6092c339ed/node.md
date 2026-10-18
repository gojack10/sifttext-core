---
id: "a472e05c-9bbf-48f2-89d7-6c6092c339ed"
name: "CLI Operations"
micro_summary: "Covers creation, retrieval, validation and recovery through the local CLI, excluding the legacy MCP surface."
status: "in_progress"
see_also: []
scope: "Use the narrow local command without confusing it with hosted sifttext-mcp or assuming customer-release tooling exists."
warnings: ""
---
For official binary installations, read [Managed Releases and Updates](../8b5361e1-20d7-42a9-9dca-26ba3bac9f25/node.md) before update operations. Its update policy supersedes the per-update consent statements in the preserved source-build procedure below. A release check only reports a newer compatible release; the agent reads that guide and runs `sifttext update` within the user's existing permissions.

## Preserved source-build procedure

Read the entire entry pipeline before operating. Use `sifttext -h` as the command reference. Input: the authorized task, code checkout if building, and a chosen knowledge home. Do not invoke hosted `sifttext-mcp` for local work or alter its credentials/configuration.

## Setup

For an installed application, set `BIN` to the absolute command path in the installer's agent-start prompt (or supplied by the owner). Run `"$BIN" runtime-check` to verify its bundled assets, then use it without rebuilding or cloning software. If the command path is missing, ask for that path; do not guess a legacy executable or change PATH/configuration. The source build below is only for an authorized source checkout, not a prerequisite for installed use.

For a source checkout, first read its root `AGENTS.md` and the native files it names. The canonical authored seed lives in the checkout's `nodes/` and `trees/`, not in hand-maintained Rust strings. Build from an authorized Core checkout with its pinned tools and checked assets; do not change a compiler, law or frozen gate to make a build pass. Run the following from that checkout, then keep the compiled command with its checked runtime assets. Copying only the executable is not a supported installation.

```sh
sh scripts/setup-tools.sh
sh scripts/build.sh
sh scripts/with-tools.sh cargo build --locked --bins
CORE="$(pwd -P)"
BIN="$CORE/target/debug/sifttext"
"$BIN" -h
# Optional local install AFTER building, into a new application directory:
# sh install-local.sh /NEW/application/directory "$HOME/sifttext"
```

In the remaining examples, `sifttext` denotes the selected verified installed or source-built command: invoke `"$BIN"`, not a legacy executable or an assumed PATH entry. It needs Git and a selected commit identity, but no hosted account, model or factory. Worktree Publication owns checked `submit`: success updates the normal home and matching retrieval and removes the completed draft. Read there before allocating the next draft; it also describes explicit `advance` recovery. `install-local.sh` copies the checked command, runtime assets and readable bundled KB into a NEW application directory, initializes the separate knowledge home (default `~/sifttext`), and prints an agent-start prompt when that home's pointer exists. It refuses an existing application path and never places software inside knowledge or knowledge inside software. Existing homes are preserved, not silently given new instructions. Install missing prerequisites within the task's existing permissions; ask the human when installation needs additional permission. Select the commit identity below before initializing. A lone executable is insufficient. This is local/source installation, not a prebuilt curl release, automatic updater, public license or universal portability promise. Approved release assets and content/license/publication review are still required for public installation.

## Select commit identity

Reuse a complete name/email already selected for this workspace or configured for its intended knowledge home. Keep existing repository-local and global Git configuration unchanged unless the owner authorizes a change. If identity is missing, ask: “Which name and email should appear in this workspace's history? You can provide your commit identity or a GitHub account, repository or commit to inspect. Otherwise, we can use SiftText Agent <agent@sifttext.com>.”

Inspect an offered GitHub source only within the task's access permissions, and confirm the discovered name/email with the human. If the human has none, chooses the default, or seems bogged down, recommend SiftText Agent <agent@sifttext.com> and ask to use it. Await the answer unless previously authorized. A bounded worker returns this question to its assigning agent. Honor a request for different attribution or omitted agent credit; Git Commit Skill owns the knowledge-commit policy.

For a NEW home, the parent must be an existing physical canonical path; `init` refuses a symlinked or aliased parent rather than writing through it. `advance` requires the entire home path to be canonical and refuses any symlink or alias along it. For an existing home, resolve its physical path with `cd -P -- "$HOME/sifttext" && pwd -P`; use that path directly and do not work around either refusal by passing an alias. Then put the selected `name` and `email` in a private temporary Git config and run one initialization command with that command-scoped selection. This example initializes the default seed; choose `--bundle` or `--empty` instead when required:

```sh
identity=$(mktemp)
chmod 600 "$identity"
git config --file "$identity" user.name "$name"
git config --file "$identity" user.email "$email"
GIT_CONFIG_GLOBAL="$identity" "$BIN" init "$home"
# For a local install, use the same command-scoped variable with install-local.sh instead.
rm "$identity"
```

Init stores the selection in the NEW repository's local config and verifies the initial commit's author and committer. Private drafts inherit that local selection. Verify with `git -C "$home" config --local --get user.name`, `git -C "$home" config --local --get user.email`, and `git -C "$home" log -1 --format='%an <%ae>'`. On refusal, retain any partial home, inspect the error, and remove only the temporary config you created. Rerunning init preserves an existing home's identity and edits.

## Initialize and allocate

If you initialized above, continue to Worktree Publication. Otherwise choose an authorized new home whose parent directory already exists and run one initialization with the complete configured identity. Run only the selected initialization, not all examples:

```sh
"$BIN" init "$home"                         # NEW independent home from the embedded first-party KB (compiled from native files)
# Alternatives, instead of the preceding command:
"$BIN" init "$alternate_home" --bundle "$bundle"  # supplied native KB
"$BIN" init "$empty_home" --empty                 # deliberately empty repository
```

Every `init` call names an explicit home; omitting both flags selects the source-owned KB, while `--bundle` and `--empty` are mutually exclusive alternatives. The CLI still requires an explicit home; use `home="$HOME/sifttext"` for the normal independent user repository, while the local installer supplies that default. Keep Core software and any optional contributor checkout outside the knowledge home. The source-owned KB is compiled directly from the readable native files and requires no private package or build checkout at runtime. Local installs carry the same readable KB; those nodes retain omissions and the bundle is not a qualified complete reviewed public distribution. A new home receives independent Git history and a verified initial content commit using the selected name/email, saves that identity repository-locally, and leaves its reader detached at that commit. Existing recognized homes are checked and left unchanged, including user edits; init is not an updater or importer. Existing unrecognized directories and symlinks are refused. On an interrupted partial initialization, retain the directory and diagnose it; do not delete it or assume rerunning will repair it. A missing Git identity returns to the selection question above.

The bundled tree is copied, never linked back into the source checkout. Updating Core does not overwrite the knowledge copy. Init seeds the home with `nodes/`, `trees/` and its own repository `.gitignore`; an `AGENTS.md` sitting in a supplied bundle is not copied. A NEW home whose copied store carries the canonical entry and its six ordered children additionally receives a generated `AGENTS.md` at the home root: ordinary file-first discovery naming `nodes/<entry>/node.md` and telling you to read each direct child's `node.md` in the order that entry records, so an agent orients with local file reads before running any command. `init` reports that file as `"pointer":"AGENTS.md"`, or `"pointer":null` when it wrote none, so `--empty` and a custom bundle without the whole pipeline never get a broken pointer.

That pointer is local discovery, not authored knowledge. It is untracked and excluded through the home's own `.git/info/exclude`, so the home still reports a clean Git state, and rerunning `init` never rewrites, reseeds or removes it; homes initialized before it existed are not retroactively given one. Because it is untracked it does not travel into a private worktree, a publication view or a fresh clone: there `sifttext instructions "$home"` remains the supported lookup, validating the store and printing the entry node plus its ordered child paths, which you read in the returned order. Startup therefore needs no SQL, no hosted tool and no guess about the entrypoint. After init, use Worktree Publication to allocate a private draft and capture its fixed baseline before edits.

## Optional Pi session association

Before installing or enabling the Pi retention extension, read [Optional Pi session retention](../../integrations/pi/README.md) for its permission boundary, exact opt-in configuration, capture behavior and optional backup/restore commands. The extension is off by default; do not change Pi settings or enable transcript retention without the user's authorization.

When the Pi retention extension is installed and `session_retention.enabled` is true in the selected home, associate a session only when it materially informed the current knowledge. Read the affected node and choose its exact UUID; do not infer or attach all sessions/nodes automatically. Ask the `sifttext_attach_session` tool to attach the current Pi session using that UUID, a short kebab-case topic, and the authorized private knowledge worktree (not the accepted home). Call once per relevant node. The tool captures and Git-checkpoints the current session, writes its tracked association and local chat link, and returns only the node and friendly attachment name—not transcript content or the source session ID. Inspect the changed worktree and continue through Worktree Publication's checks and checked submission. If retention is disabled, the source is unavailable, or the tool fails, do not fabricate a descriptor or block ordinary knowledge work; report the limitation.

## Scratch creation

For an already-approved scratch creation, set `wt` to the allocated private worktree:

```sh
sifttext create "$wt" 'Scratch question' 'Keeps a reversible trial separate from accepted knowledge.'
# Use the actual returned root UUID below:
sifttext create "$wt" 'Trial note' 'Records evidence from the bounded local trial.' "$root_uuid"
```

The helper creates canonical UUID directories, node files, child ordering and root/child symlinks, normalizes affected statuses, and validates a staged candidate before applying it. It returns the UUID/path and does not commit. It rejects creation outside a private Git worktree. The current helper assumes one cooperative writer to that draft; after partial I/O failure preserve and inspect it rather than claiming atomic multi-file publication. Use normal file tools to read/edit the body, not a separate text-edit command vocabulary.

Continue through Worktree Publication's before/after checks and explicit commit/publication steps. Use the retrieval children to demonstrate that the resulting nodes can be found and read. If source validation fails, stop dependent retrieval and repair the authorized draft; do not treat stale cached results as success.

## Boundaries and return

This is local/source installation, not a public signed curl installer/updater. The current seed retains inherited omissions; do not claim complete reviewed public distribution from a successful copy. Official methodology/Core knowledge updates require consent and preservation of user edits; no silent reseed or cloud sync is provided. No private migration or limit increase is authorized by this procedure.

Return the actual command/install path, home/worktree paths, created UUIDs, check results and commit boundary reached. Existing-home idempotence, installed operation, recovery and fresh-agent success require observations; do not infer them from the commands listed here.
