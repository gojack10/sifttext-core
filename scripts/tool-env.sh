# shellcheck shell=sh
# Shared contributor tool environment. Source after ROOT is set so that setup,
# build and every check command resolve the same toolchain. A tool installed by
# scripts/setup-tools.sh lives under $ROOT/.tools and is never exported to a
# shell, so later direct Cargo calls must go through this file or
# scripts/with-tools.sh.

tool_env_apply() {
  # Probing a missing pinned toolchain must not install into ambient user homes.
  RUSTUP_AUTO_INSTALL=0
  export RUSTUP_AUTO_INSTALL
  if [ -d "$ROOT/.tools/cargo/bin" ]; then
    CARGO_HOME=$ROOT/.tools/cargo
    RUSTUP_HOME=$ROOT/.tools/rustup
    PATH=$CARGO_HOME/bin:$PATH
    export CARGO_HOME RUSTUP_HOME PATH
  fi
}

# Bend 2.0.27's native-code generation needs Clang 14+, not just a C compiler.
tool_env_verify_clang() {
  VERSION=$(clang --version 2>/dev/null | awk 'NR == 1 { for (i = 1; i <= NF; i++) if ($i == "version") { print $(i + 1); exit } }') || return 1
  MAJOR=${VERSION%%.*}
  case "$MAJOR" in ''|*[!0-9]*) return 1 ;; esac
  [ "$MAJOR" -ge 14 ]
}

# A named Rustup toolchain can report the right version and still be incomplete
# (missing manifest), so prove it compiles and links instead of trusting
# --version. The link probe also exposes a missing C compiler or linker.
tool_env_verify_rust() {
  rustc --version 2>/dev/null | grep -q '^rustc 1\.95\.0 ' || return 1
  cargo --version 2>/dev/null | grep -q '^cargo 1\.95\.0' || return 1
  PROBE=$(mktemp -d "${TMPDIR:-/tmp}/sifttext-rust-probe.XXXXXX") || return 1
  printf 'fn main() {}\n' > "$PROBE/probe.rs" || { rm -rf "$PROBE"; return 1; }
  if ! rustc -o "$PROBE/probe" "$PROBE/probe.rs" 2> "$PROBE/probe.err"; then
    cat "$PROBE/probe.err" >&2
    rm -rf "$PROBE"
    return 1
  fi
  "$PROBE/probe"
  STATUS=$?
  rm -rf "$PROBE"
  [ "$STATUS" -eq 0 ] || return 1
}
