#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
VERSION=${1:?usage: package-release.sh VERSION OUTPUT}
OUTPUT=${2:?usage: package-release.sh VERSION OUTPUT}
case "$VERSION" in ''|*[!a-zA-Z0-9._-]*|.|..) echo 'Invalid release version' >&2; exit 1 ;; esac
mkdir -p "$OUTPUT"
OUTPUT=$(CDPATH= cd -- "$OUTPUT" && pwd)
. scripts/tool-pins.sh
platform_pins
SOURCE=$(git rev-parse HEAD)
KB=$(git rev-parse HEAD:nodes HEAD:trees | shasum -a 256 | cut -d ' ' -f 1)
[ -z "$(git status --porcelain --untracked-files=all)" ] || { echo 'Release source must be committed and clean.' >&2; exit 1; }
sh scripts/build.sh --release
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
mkdir -p "$STAGE/package"
for name in Cargo.toml Cargo.lock build.rs README.md AGENTS.md .gitignore rust-toolchain.toml install-local.sh src laws scripts tests fixtures docs nodes trees; do
  [ ! -e "$name" ] || cp -R "$name" "$STAGE/package/$name"
done
mkdir "$STAGE/package/build"
for name in graph-policy graph-policy.sha256 graph-transition graph-transition.sha256 candidate.sha256; do cp "build/$name" "$STAGE/package/build/$name"; done
cp target/release/sifttext "$STAGE/package/sifttext"
printf '{"version":"%s","source":"%s","knowledge":"%s","format":1}\n' "$VERSION" "$SOURCE" "$KB" > "$STAGE/package/release.json"
# Normalize archive order, times and ownership with GNU tar on every build host.
TAR=tar
[ "$(uname -s)" != Darwin ] || TAR=gtar
EPOCH=$(git show -s --format=%ct HEAD)
cp "$STAGE/package/release.json" "$OUTPUT/release-info.json"
"$TAR" --sort=name --mtime="@$EPOCH" --owner=0 --group=0 --numeric-owner --format=gnu -C "$STAGE" -cf "$STAGE/archive.tar" package
gzip -n -c "$STAGE/archive.tar" > "$OUTPUT/sifttext-$PLATFORM.tar.gz"
mkdir "$STAGE/kb"
cp -R nodes trees "$STAGE/kb/"
rm -rf "$STAGE/package"
mv "$STAGE/kb" "$STAGE/package"
"$TAR" --sort=name --mtime="@$EPOCH" --owner=0 --group=0 --numeric-owner --format=gnu -C "$STAGE" -cf "$STAGE/archive.tar" package
gzip -n -c "$STAGE/archive.tar" > "$OUTPUT/sifttext-knowledge.tar.gz"
printf '%s\n' "$KB" > "$OUTPUT/knowledge-revision.txt"
