# SiftText

> Build an understanding your agents can keep working from.

We believe agents will do far more of our work, so people need a way to question their decisions without following every conversation. We're building SiftText to help your agents preserve the reasons behind those decisions across sessions, with you free to inspect or redirect the work. As circumstances change and models improve, we want that record to help future agents question earlier assumptions and discover possibilities we missed.

## Get started

Give your agent this installation request:

> Install SiftText for local project memory by following https://github.com/gojack10/sifttext-core/blob/main/INSTALL.md and handle setup within my permissions.

After setup, your agent reads the installed instructions before continuing your project or asking what you would like to work on.

For direct terminal installation, follow [INSTALL.md](INSTALL.md) to inspect and verify the downloaded installer before executing that same file.

## What installation does

The precompiled [v0.1.0 release](https://github.com/gojack10/sifttext-core/releases/tag/v0.1.0) supports macOS 15+ and Linux with glibc 2.35+ on arm64 or x86_64. With Git and the download tools available, installation puts the application at `~/.local/share/sifttext` and initializes knowledge at `~/sifttext`. Keeping those locations separate from project code lets your knowledge survive software replacement.

Managed v0.1.0 installations check official releases opportunistically after four hours and report a newer compatible release instead of applying it; your agent runs `sifttext update` under the standing permission you record with `sifttext update --grant standing`; `sifttext update --config` shows the current mode, grant and file path, and nothing is granted until you record it. Those commands ship with the next official release; the published v0.1.0 accepts only `update`, `--status`, `--resume` and `--rollback`, so on v0.1.0 your agent asks you before each update. To skip the background release check, run `sifttext update --mode disabled` once it is available or have your agent use `SIFTTEXT_NO_UPDATE=1` for installation and every later invocation.

Continue local work during a blocked update while your agent reconciles the knowledge merge in its retained private worktree within your permission.

## Inspect a release

Each [release record](https://github.com/gojack10/sifttext-core/releases/tag/v0.1.0) includes SHA256SUMS with the source revision used to build its packages. Follow that revision in the [source repository](https://github.com/gojack10/sifttext-core) to inspect the installer, then use [INSTALL.md](INSTALL.md) to compare the downloaded bytes and verify their checksum before execution.

The checksums depend on the publisher's GitHub account and HTTPS delivery. A checksum shows the package matches the bytes the official release serves; it does not show which build produced them. Because v0.1.0 is unsigned and unnotarized, users whose policy requires signed build provenance must wait for a release that supplies it. The installer needs only curl, tar and Git: it never requires GitHub CLI, and it neither downloads nor bootstraps a verifier of its own.

## How it works

Knowledge lives in local Markdown files with Git history. Related questions form branches that agents explore through outlines and search, using the bundled instructions to find the relevant research as a project develops.

To change that knowledge, an agent edits in a private Git worktree and submits it for validation of the combined structure. Accepted changes remain in Git history for later inspection or recovery.

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

Choose a new absolute application path whose parent already exists, with no symlink components. To keep knowledge outside the application, use the default `~/sifttext` or supply another absolute path as the installer's second argument. Once it recognizes or initializes that home, the installer prints a handoff using the resulting local paths.

Source-build and installed workflows were tested on macOS 26.4/arm64 and Ubuntu 24.04.4/aarch64 at `991aca3`.

### Tests

After building:

```sh
sh scripts/with-tools.sh cargo test --locked
sh scripts/law-mutations.sh
```

For command help, run the installed `sifttext -h`.

## License

[PolyForm Perimeter 1.0.1](LICENSE) permits source access while restricting competing products. Upstream components retain their own terms, including Apache-2.0 for [Leviathan](https://github.com/elstongun/leviathan) and the MIT notices in bundled skills.
