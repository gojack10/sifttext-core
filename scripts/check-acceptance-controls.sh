#!/bin/sh
# Compile the stdlib-only preflight without Cargo, Bend or tool installation.
set -eu
cd "$(dirname "$0")/.."
ROOT=$(CDPATH= cd -- . && pwd)
RUSTUP_AUTO_INSTALL=0
export RUSTUP_AUTO_INSTALL
if [ -d "$ROOT/.tools/cargo/bin" ]; then
  CARGO_HOME=$ROOT/.tools/cargo
  RUSTUP_HOME=$ROOT/.tools/rustup
  PATH=$CARGO_HOME/bin:$PATH
  export CARGO_HOME RUSTUP_HOME PATH
fi
CHECK=$(mktemp -d "${TMPDIR:-/tmp}/sifttext-acceptance-check.XXXXXX")
trap 'rm -rf "$CHECK"' EXIT
trap 'exit 1' HUP INT TERM
rustc --edition=2024 -D warnings scripts/acceptance-controls.rs -o "$CHECK/check"
"$CHECK/check" "$@"
