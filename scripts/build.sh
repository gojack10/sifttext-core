#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
ROOT=$(CDPATH= cd -- . && pwd)
# Fail stale acceptance selectors before any tool setup or expensive compilation.
sh "$ROOT/scripts/check-acceptance-controls.sh"
. "$ROOT/scripts/tool-env.sh"
tool_env_apply
tool_env_verify_clang || { echo 'Bend 2.0.27 requires clang 14 or newer (install with user approval).' >&2; exit 1; }
tool_env_verify_rust || { echo 'Rust 1.95.0 with a working linker is required; run sh scripts/setup-tools.sh' >&2; exit 1; }
BEND=$(sh scripts/bend-path.sh)
shasum -a 256 -c laws/frozen.sha256
mkdir -p build
# A failed checker must never leave a fresh acceptance stamp.
rm -f build/candidate.sha256 build/graph-policy.sha256 build/graph-transition.sha256
"$BEND" laws/PROOF.bend --check-only
"$BEND" laws/Runtime.bend --check-only
"$BEND" laws/successor/validation/Graph.bend --check-only
"$BEND" laws/successor/ValidationRuntime.bend --check-only
"$BEND" laws/successor/ValidationRuntime.bend -o build/graph-policy
shasum -a 256 build/graph-policy > build/graph-policy.sha256
"$BEND" laws/successor/GraphTransition.bend --check-only
"$BEND" laws/successor/GraphTransition.bend -o build/graph-transition
shasum -a 256 build/graph-transition > build/graph-transition.sha256
cargo build --locked
{ printf '%s\n' Cargo.toml Cargo.lock README.md .gitignore rust-toolchain.toml;
  for dir in src laws scripts tests fixtures docs; do
    if [ -d "$dir" ]; then find "$dir" -type f | LC_ALL=C sort; fi
  done
} |
  while IFS= read -r file; do shasum -a 256 "$file"; done > build/candidate.sha256
