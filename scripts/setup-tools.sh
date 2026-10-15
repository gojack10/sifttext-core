#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
. "$ROOT/scripts/tool-pins.sh"
. "$ROOT/scripts/tool-env.sh"
platform_pins
for tool in curl tar git cc shasum clang; do
  command -v "$tool" >/dev/null || { echo "Missing prerequisite: $tool (install with user approval)" >&2; exit 1; }
done
tool_env_verify_clang || { echo 'Bend 2.0.27 requires clang 14 or newer (install with user approval).' >&2; exit 1; }
mkdir -p "$ROOT/.tools"
# Never replace an existing tool directory, even if an interrupted setup left it invalid.
if [ ! -e "$ROOT/.tools/bend-$PLATFORM" ]; then
  STAGE=$(mktemp -d "$ROOT/.tools/acquire.XXXXXX")
  trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
  curl --fail --location --show-error --silent --proto '=https' --proto-redir '=https' \
    "https://github.com/bendlang/bend/releases/download/v2.0.27/bend-2.0.27-$PLATFORM.tar.gz" -o "$STAGE/bend.tar.gz"
  [ "$(tool_sha256 "$STAGE/bend.tar.gz")" = "$ARCHIVE_SHA256" ] || { echo 'Bend archive digest mismatch' >&2; exit 1; }
  tar -xzf "$STAGE/bend.tar.gz" -C "$STAGE"
  [ "$(tool_sha256 "$STAGE/bend/bin/bend")" = "$BEND_SHA256" ] || { echo 'Bend binary digest mismatch' >&2; exit 1; }
  rm "$STAGE/bend.tar.gz"
  mv "$STAGE" "$ROOT/.tools/bend-$PLATFORM"
  trap - EXIT HUP INT TERM
fi
sh "$ROOT/scripts/bend-path.sh" >&2
tool_env_apply
if tool_env_verify_rust; then
  printf '%s\n' 'Using the Rust 1.95.0 toolchain already on PATH.'
else
  # An unusable or incomplete toolchain is replaced only inside the repository,
  # never in a user or system Rust installation.
  [ ! -e "$ROOT/.tools/cargo" ] && [ ! -e "$ROOT/.tools/rustup" ] || {
    echo 'Repository-local Rust directories exist but are unusable; inspect .tools without overwrite.' >&2
    exit 1
  }
  export CARGO_HOME="$ROOT/.tools/cargo" RUSTUP_HOME="$ROOT/.tools/rustup"
  STAGE=$(mktemp -d "$ROOT/.tools/rustup.XXXXXX")
  trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
  URL="https://static.rust-lang.org/rustup/archive/1.28.2/$RUST_TARGET/rustup-init"
  curl --fail --location --show-error --silent --proto '=https' --proto-redir '=https' "$URL" -o "$STAGE/rustup-init"
  curl --fail --location --show-error --silent --proto '=https' --proto-redir '=https' "$URL.sha256" -o "$STAGE/rustup-init.sha256"
  read -r DIGEST REST < "$STAGE/rustup-init.sha256"
  [ "$(tool_sha256 "$STAGE/rustup-init")" = "$DIGEST" ] || { echo 'Official Rustup digest mismatch' >&2; exit 1; }
  chmod +x "$STAGE/rustup-init"
  "$STAGE/rustup-init" -y --no-modify-path --profile minimal --default-toolchain 1.95.0
  tool_env_apply
  tool_env_verify_rust || { echo 'Rust 1.95.0 still unusable after repository-local acquisition' >&2; exit 1; }
fi
printf '%s\n' 'Tools prepared; run sh scripts/build.sh, or prefix any check with sh scripts/with-tools.sh. No shell/system configuration or existing knowledge was changed.'
