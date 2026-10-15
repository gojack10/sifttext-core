#!/bin/sh
# Install this checked source build into a NEW application directory, then init
# an independent knowledge home. No downloads, sudo, PATH/config edits or updates.
set -eu
[ "$#" -ge 1 ] && [ "$#" -le 2 ] || {
  echo 'usage: sh install-local.sh NEW_APPLICATION_DIR [KNOWLEDGE_HOME]' >&2
  exit 1
}
cd "$(dirname "$0")"
command -v git >/dev/null || { echo 'Git is required; install it before continuing.' >&2; exit 1; }
prefix=$1
home=${2:-"$HOME/sifttext"}
case "$prefix" in /*) ;; *) echo 'application directory must be absolute' >&2; exit 1;; esac
case "$home" in /*) ;; *) echo 'knowledge home must be absolute' >&2; exit 1;; esac
# Refuse aliases and nested software/knowledge paths before writing anything.
[ "$(cd "$(dirname "$prefix")" && pwd -P)/$(basename "$prefix")" = "$prefix" ] || {
  echo 'application directory needs a canonical existing parent' >&2; exit 1;
}
[ "$(cd "$(dirname "$home")" && pwd -P)/$(basename "$home")" = "$home" ] || {
  echo 'knowledge home needs a canonical existing parent' >&2; exit 1;
}
case "$prefix/" in "$home/"*) echo 'software and knowledge must be separate' >&2; exit 1;; esac
case "$home/" in "$prefix/"*) echo 'software and knowledge must be separate' >&2; exit 1;; esac
# Validate the actual seed and checked assets BEFORE creating the prefix.
./target/debug/sifttext runtime-check >/dev/null
# mkdir refuses existing directories and symlinks; never merge/overwrite a prefix.
mkdir -m 700 "$prefix"
# Only remove the application we just created; preserve even an incomplete home.
trap 'rm -rf "$prefix"' 0
trap 'exit 1' 1 2 15
cp -R Cargo.toml Cargo.lock build.rs install-local.sh README.md AGENTS.md .gitignore rust-toolchain.toml src laws scripts tests fixtures nodes trees "$prefix/"
# Optional checked documentation belongs to the stamped source inventory too.
if [ -d docs ]; then cp -R docs "$prefix/"; fi
# /tmp-backed build symlinks are source outputs, not installed dependencies.
mkdir "$prefix/build"
cp -R build/. "$prefix/build/"
cp target/debug/sifttext "$prefix/sifttext"
"$prefix/sifttext" runtime-check >/dev/null
"$prefix/sifttext" init "$home"
trap - 0 1 2 15
if [ -f "$home/AGENTS.md" ] && [ ! -L "$home/AGENTS.md" ]; then
  printf '\nTell your agent: Read "%s/AGENTS.md" and use "%s/sifttext" for this project.\n' "$home" "$prefix"
else
  printf '\nExisting home preserved. Run "%s/sifttext" instructions "%s" for its authored entry paths.\n' "$prefix" "$home"
fi
