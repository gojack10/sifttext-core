# Ordinary Git compatibility: optional acceleration

Core knowledge remains an ordinary Git repository. Normal status must continue
showing tracked edits, deletions and new untracked authored files. Ignoring
outputs does not necessarily avoid enumerating their containing directories.

## Publication scratch lifetime

Checked publication reserves each private scratch view and syncs its versioned
ownership record before asking Git to register it. Disposable views and returned
retained reading views are distinct: retention is durable before a view is
returned. Only a matching recorded disposable view is eligible for next-operation
recovery. Ordinary `read`/`search` do not initiate cleanup; unrecorded legacy,
retained, replaced, branch-owned, unexpectedly edited or ambiguous views stay put.

Controlled publication/reader Git helpers have a 30-second per-helper deadline,
output limits, a Unix process group and inherited advisory custody for their
ordinary descendants. Timeout/error handling kills and drains owned execution
before cleanup. Core death leaves recorded ownership and inherited custody, not
an RAII cleanup guarantee. The next explicit checked publication tries each
recorded path without waiting on busy scratch. This is cooperative local Unix
custody, not a sandbox against a substituted executable, escaped session/closed
handles, arbitrary filesystem writers, power loss or storage failure.

Failure receipts retain a bounded `failure_kind` and `lifecycle` cleanup notes;
unusable scratch is never returned as a verified `view`. Cleanup refusal preserves
the directory and registration together for inspection/retry. Keep the original
submit arguments and save journals after interruption: `not_accepted`,
`accepted_incomplete` and `unknown` remain different outcomes. Never remove locks,
unlock/prune arbitrary registrations or infer old ownership from age/PID absence.

## Supported opt-in

This optional setup uses **both** standard Git `core.fsmonitor=true` (the built-in
per-repository daemon, not an external helper) and `core.untrackedCache=true`,
then deliberately runs an index-writing status. It is **not** enabled by Core
initialization, installation, update or reads. It changes only the selected
repository's two local config keys and Git's acceleration/index metadata; it
neither stages files nor changes authored bytes, refs or ignore rules.

Qualified here: Git **2.53.0**, macOS/arm64, **local internal APFS**, an independent
repository with its own ordinary `.git` directory/config/index. The setup checks
macOS, internal APFS, Git >=2.36 with the advertised `fsmonitor--daemon` feature,
and `git update-index --test-untracked-cache` on the actual filesystem. Other
capable macOS Git versions still require a local visibility/performance check;
passing the preflight is not a performance guarantee. No Linux, Windows,
network/external filesystem or linked-worktree qualification is claimed. Git's
own platform support is broader than this bounded Core setup path.

Before opting in:

1. Obtain permission to change this repository's config and run its daemon.
   Finish active Git/config/index writers; keep other writers stopped during
   setup/re-prime/rollback. Do not remove another operation's locks.
2. Inspect **every Git client** that will touch the repository, including IDEs,
   harnesses and alternate executables: run its `git version --build-options`.
   Each must be >=2.36 and advertise `feature: fsmonitor--daemon`, or explicitly
   disable fsmonitor for its commands using a modern compatible Git. Do not
   opt in with unknown/older clients. Git <=2.35.1 can interpret boolean values
   as helper paths; particularly old versions can miss changes. The flag below
   is the operator's assertion that this review is complete, not an automatic
   discovery of hidden client executables.
3. Use an absolute physical repository-root path (`pwd -P`); no symlink aliases.
   This path must be the independent canonical knowledge repository, **not**
   one of Core's private editing/reader worktrees.
4. Review existing configuration. The bounded path accepts either **neither**
   accelerator key anywhere in effective config, or exactly one literal `true`
   for each in this repository's own config. It refuses custom monitor paths,
   inherited/duplicate/empty/false/mixed settings, local includes, worktree
   config and fsmonitor remote/socket overrides rather than replacing them.
   Review those separately; do not erase custom settings just to pass setup.
   Git environment redirects are refused except `GIT_OPTIONAL_LOCKS`, which the
   priming status deliberately overrides to `1`.

From a source checkout, or the application directory containing `scripts/`:

```sh
sh scripts/git-acceleration.sh enable /absolute/physical/knowledge-home --all-git-clients-reviewed
```

This is the supported explicit setup entry, not a new `sifttext` subcommand.
It is also safe to repeat with the exact already-enabled local pair: no duplicate
config entries are added. It prints the actual full-repository porcelain status
(including normal untracked visibility), so a dirty home remains visibly dirty.
There is no model/harness launch, global config edit, auto-maintenance, backup,
external monitor installation, service registration or polling loop.

### Failure and rollback

A failed preflight leaves config/daemon untouched. After preflight, setup starts
Git's daemon only if none was running. If daemon startup, config setup or priming
fails, it exits nonzero, removes **only config keys added by that invocation**,
and stops **only a daemon it started**. A pre-existing daemon and pre-existing
true/true settings are preserved on failure. Cleanup errors are printed and
require inspection; abrupt SIGKILL/power loss cannot guarantee rollback. A
failed priming command may have refreshed acceleration metadata in the index;
setup does not restore an old whole index/config over newer user work. No
tracked/staged selection is intentionally changed. Do not run concurrent writers
and assume this sequence is a cross-process transaction.

To abandon a successful opt-in, separately authorize removing the two local keys
**only if they are still the pair this setup added**, and stopping the repository
daemon **only if you own its lifecycle**. Use normal `git config --local --unset`
commands, not a whole-config restore. Do not stop someone else's daemon. For a
one-off unaccelerated diagnostic that cannot execute a custom monitor:

```sh
GIT_OPTIONAL_LOCKS=0 git -c core.fsmonitor=false -c core.untrackedCache=false status --porcelain
```

## Explicit re-prime, not every-turn refresh

Config alone is insufficient for a workload that always uses
`GIT_OPTIONAL_LOCKS=0`: it may never persist the acceleration state in the index.
After daemon restart/event-history invalidation, index replacement, a client
removing index extensions, or an observed return to expensive scans, run once:

```sh
sh scripts/git-acceleration.sh reprime /absolute/physical/knowledge-home --all-git-clients-reviewed
```

Re-prime requires the already-enabled local pair and repeats capability/filesystem
checks and one writable-index status. It does not continually re-seed, alter Core
read/publication isolation, or guarantee that another Git writer retains the
extensions. Core deliberately disables fsmonitor in safety-sensitive reader and
checked-publication commands; those paths are **not accelerated or repaired** by
this opt-in. Re-prime after a writer invalidates acceleration state when needed;
do not add a heavy automatic refresh to every turn.

## Evidence and limits

On a disposable copy of 65,673 tracked entries and 65,672 ignored generated files,
Git 2.53.0 on local macOS APFS took 580–763ms wall / 1.42–1.76 CPU-seconds for warm
unaccelerated ordinary status. Both facilities, after an index-writing seed,
took 43–56ms / 0.01–0.03 CPU-seconds; a released-Codex-equivalent no-optional-locks
status replay took 49–55ms / 0.03 CPU-seconds. Config-only initial no-write calls
still took 1.05–1.16s / 1.79–1.82 CPU-seconds. After a daemon restart, CPU remained
higher until explicit re-prime. Setup/re-prime is not a cheap per-turn operation.
These are bounded warm measurements, not a latency SLA or storage-cold result.
CPU figures measure the Git command, **not detached daemon CPU/idle cost**, which
was not benchmarked; no total-system energy claim follows.

External tracked edits, authored additions/deletions, structural links/orders,
branch changes and daemon restarts matched independently unaccelerated status in
the experiment. The final setup path was additionally exercised on disposable
fixtures with staged/unstaged changes, external writes and restart/re-prime;
custom config, unsupported capability and linked-worktree refusal, and injected
config/seed failure rollback were checked. No real knowledge-home adoption or
installed Codex executable/model run is implied.

The inspected Codex release `rust-v0.163.0` keeps boolean built-in fsmonitor when
Git advertises support, rejects helper paths, and sets `GIT_OPTIONAL_LOCKS=0`.
That policy explains both compatibility and the need for explicit seeding; it is
not maintainer-confirmed integration support for every harness version.

Sources:

- Git 2.53.0 installed `git-fsmonitor--daemon`, `git-update-index` (Untracked Cache
  and File System Monitor), and `git-config` (`core.fsmonitor`,
  `core.untrackedCache`) documentation. See also the corresponding
  [Git manual](https://git-scm.com/docs/git-fsmonitor--daemon).
- Codex released commit
  [`83086188ae0614bd882818ff1aa23f3194962012`, fsmonitor policy](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/git-utils/src/fsmonitor.rs#L1-L137)
  and [command wrapper](https://github.com/openai/codex/blob/83086188ae0614bd882818ff1aa23f3194962012/codex-rs/git-utils/src/info.rs#L370-L428).
