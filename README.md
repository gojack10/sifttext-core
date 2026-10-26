# SiftText

## Get started

Install from your terminal:

```sh
curl -fsSL https://sifttext.com/install | sh
```

Open a new shell if `sifttext` is not yet on PATH.

Tell your agent to run `sifttext instructions` and read the returned entry file before continuing your project.

## Plans

The [website](https://sifttext.com/#plans) offers these plans:

| Plan | Price (USD) | Included offering |
| --- | --- | --- |
| Core | Free | Local Markdown files with Git history for agents with file and terminal access. |
| [Plus](https://app.sifttext.com/subscribe?plan=plus) | $20/month | Core, hosted MCP access, and cloud sync. |
| [Pro](https://app.sifttext.com/subscribe?plan=pro) | $50/month | Plus, an interactive browser through MCP, writing checks, and automatic tree organization checks. |

For setup details or an inspect-before-run option, follow [INSTALL.md](INSTALL.md).

### Example

- **Choose a dependable supplier** (in progress)
  Customer orders need a supplier we can rely on, so compare quotes and check performance on real orders.

  - **Use Company A for every order** (discarded)
    Both suppliers’ samples passed inspection, so we chose A at $8.20/unit over B at $10.
    That choice assumed A’s five-day delivery estimate was reliable. The later delivery review ruled out A for urgent orders.

    - **Review Company A’s deliveries** (resolved)
      Three of five orders arrived late. Two delayed customer shipments.
      The lower price no longer justifies relying on A for urgent work.

  - **Trial Company B for urgent orders** (new decision)
    A’s delivery review exposed missed customer deadlines. B quotes three days, but its delivery performance remains untested.
    Trial 100 units with B. Keep A for flexible orders.

    - **Check Company B’s trial results** (not examined)
      No trial results yet.
      Check delivery dates and quality before giving B more orders.

## Running agents across a team?

Bring your team’s work into one place for people and agents.

[Book a demo](https://cal.com/jack-ten-bosch/30min)

## How it works

Knowledge lives in local Markdown files with Git history. Related questions form branches that agents explore through outlines and search, using the bundled instructions to find the relevant research as a project develops.

To change that knowledge, an agent edits ordinary files in a private Git worktree, reviews and commits the intended paths, then submits the commit and its base SHA. Submit normalizes statuses and checks the exact combined candidate before updating accepted main. Accepted changes remain in Git history; incomplete drafts remain recoverable. Semantic checking is deferred, not reported as passed.

`worktree HOME` allocates a sparse draft without a full checkout. Expand an existing or new node with `git -C WORKTREE sparse-checkout add nodes/UUID`; this does not pull a newer version. `sifttext create` selects its new node and parent automatically. Then edit and stage ordinary authored files in that draft. For manually created files outside the selection, select their directory first (or deliberately stage with `git add --sparse`). Read an omitted node with `sifttext read WORKTREE UUID` without materializing it. SQL, search, outlines and submit use the complete logical version, including omitted Git content and valid uncommitted changes; missing sparse paths are not deletions. Submit materializes only nodes requiring status patches. `git -C WORKTREE sparse-checkout disable` is the full-draft fallback. The normal home stays complete; keep durable task artifacts there, not in the disposable draft.

## What installation does

Precompiled packages and supported platforms are listed on the [latest release](https://github.com/gojack10/sifttext-core/releases/latest). By default, installation puts the application at `~/.local/share/sifttext` and initializes knowledge at `~/sifttext`. Keeping those locations separate from project code lets your knowledge survive software replacement.

To upgrade a healthy managed installation, use `sifttext update` within your update permission and follow the installed update guide. Rerunning the installer leaves the existing application in place.

Continue local work during a blocked knowledge merge while your agent reconciles the retained private worktree within your permission.

## Ordinary Git compatibility

For large homes, `scripts/git-acceleration.sh`
can reduce warm status work using Git's built-in monitor and untracked cache.
It is opt-in, repository-local, capability/filesystem gated, and currently
qualified only on local internal macOS APFS. Initialization does not enable it;
setup and re-prime deliberately allow an index write without staging user files.
It does not repair or accelerate Core's isolated checked-publication helpers.

## Inspect a release

Each [official release](https://github.com/gojack10/sifttext-core/releases/latest) publishes SHA256SUMS with its source revision. Follow [INSTALL.md](INSTALL.md) to compare the downloaded installer with that source and verify its published checksum before execution. A matching checksum establishes byte agreement with the release record. Build provenance requires separate evidence.

The installer’s complete tool requirements are Git, curl, tar and an existing `shasum` or `sha256sum`.

## Development

To inspect or adapt Core under the [PolyForm Perimeter license](LICENSE), begin with [AGENTS.md](AGENTS.md) and its local instruction pipeline. That route lets an agent handle an authorized source-development assignment from the checkout.

### Install from source

From a checkout of this repository:

```sh
sh scripts/setup-tools.sh
sh scripts/build.sh
sh install-local.sh "$HOME/sifttext-core-app"
```

Prerequisites are Git, curl, tar, shasum, a C compiler/linker and Clang 14+. Use Xcode Command Line Tools on macOS. On Ubuntu/Debian, install `clang build-essential git curl ca-certificates perl` within your installation permissions.

Setup acquires verified [Bend](https://github.com/bendlang/bend) 2.0.27 and Rust 1.95.0 into checkout-local `.tools/` when needed. Before initialization, select a Git name and email for the knowledge repository using the identity procedure in the local instructions.

Choose a new absolute application path whose parent already exists, with no symlink components. To keep knowledge outside the application, use the default `~/sifttext` or supply another absolute path as the installer's second argument.

Source-build and installed workflows were qualified on macOS 26.4/arm64 and Ubuntu 24.04.4/aarch64 at historical source pin `991aca3`; that does not qualify this revision on Linux or establish official harness integration support.

### Qualify a change

Keep pinned builds, frozen-law verification, Bend formal checks and source/runtime stamps enabled. The build checks include the input-normalization proof in `Shape.bend`; formal assurance concerns the modeled graph, not all Rust, filesystem behavior or security. Body token counting retains the pinned `o200k_base` vocabulary, regex and upstream BPE semantics.

Use the public CLI in disposable homes to exercise the changed workflow, refusals and recovery. Use ad hoc inspection, a native debugger and native CPU/RSS/process measurements as needed; preserve exact argv, source/runtime identity and outputs. Do not substitute a test-suite run or a text walkthrough for ordinary-use evidence. Current full-submit and warm-SQL measurements do not meet the resource/latency targets; no universal 50 MB or instant-query claim is made.

For operations and flags, use the built or installed `sifttext -h`.

## License

[PolyForm Perimeter 1.0.1](LICENSE) permits source access while restricting competing products. Upstream components retain their own terms, including Apache-2.0 for [Leviathan](https://github.com/elstongun/leviathan) and the MIT notices in bundled skills.
