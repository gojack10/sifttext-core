# SiftText

Build an understanding you can keep working from.

Use SiftText with your AI agent to investigate a project over time. Each finding connects to the question that produced it. When new evidence challenges an earlier decision, you can follow how the thinking changed and continue the investigation.

Suppose your agent recommends a supplier for its low prices. When deliveries start arriving late, it can revisit the choice using the original research.

Your agents maintain the knowledge. You can inspect it and steer the work when you want.

## Get started

**Status:** source-build alpha. The curl installer and final Knowledge Base bundle are pending.

Give an agent with local file and terminal access this prompt:

> Follow https://github.com/gojack10/sifttext-core/blob/main/AGENTS.md to set up SiftText for [my project]. Use it to maintain our developing understanding as we work. Ask before installing tools that need my permission.

**Agents:** read [AGENTS.md](AGENTS.md).

## How it works

Knowledge lives in local Markdown files with Git history. Related questions form branches that agents explore through outlines and search. The bundled instructions guide agents in maintaining those relationships as work develops.

Agents edit in private Git worktrees. Checked submission validates the combined structure before accepting a change. Git history supports inspection and recovery.

The knowledge repository stays separate from your project code and Core installation. Installed use requires Git and the complete application directory.

## Development

### Install from source

From a checkout of this repository:

```sh
sh scripts/setup-tools.sh
sh scripts/build.sh
sh install-local.sh "$HOME/sifttext-core-app"
```

Prerequisites: Git, curl, tar, shasum, a C compiler/linker and Clang 14+. On macOS, use Xcode Command Line Tools. On Ubuntu/Debian, install `clang build-essential git curl ca-certificates perl`.

Setup downloads verified [Bend](https://github.com/bendlang/bend) 2.0.27 and Rust 1.95.0 as needed into checkout-local `.tools/`. Initialization requires a selected Git name and email. Your agent can help configure that identity for the knowledge home.

Choose a new application directory. Knowledge defaults to `~/sifttext`, or an absolute path supplied as the installer's second argument. Keep these destinations separate, with existing canonical parent directories. Recognized knowledge homes and existing edits are preserved.

The installer prints a prompt with your local paths. Give it to your agent and start working on your project.

Source-build and installed workflows were tested on macOS 26.4/arm64 and Ubuntu 24.04.4/aarch64 at `991aca3`.

### Tests

After building:

```sh
sh scripts/with-tools.sh cargo test --locked
sh scripts/law-mutations.sh
```

Run the installed `sifttext -h` for command help.

## License

[PolyForm Perimeter 1.0.1](LICENSE) permits source access and restricts providing competing products. [Leviathan](https://github.com/elstongun/leviathan) retains its Apache-2.0 terms. Bundled upstream skills retain their MIT notices.
