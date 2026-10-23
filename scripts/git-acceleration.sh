#!/bin/sh
# Explicit ordinary-Git acceleration; never called by init/install or readers.
set -eu
fail() { printf '%s\n' "$*" >&2; exit 1; }
[ "$#" = 3 ] || fail 'usage: sh scripts/git-acceleration.sh enable|reprime CANONICAL_REPO --all-git-clients-reviewed'
mode=$1
repo=$2
[ "$3" = --all-git-clients-reviewed ] || fail 'Review every participating Git client before opting in.'
case "$mode" in enable|reprime) ;; *) fail 'Expected enable or reprime.';; esac
[ "$(uname -s)" = Darwin ] || fail 'Qualified only on macOS with local internal APFS.'
# Repository/environment redirects could target another index or shared config.
[ -z "$(env | grep '^GIT_' | grep -v '^GIT_OPTIONAL_LOCKS=' || :)" ] || fail 'Unset Git environment overrides first (except GIT_OPTIONAL_LOCKS).'
case "$repo" in /*) ;; *) fail 'Repository must be an absolute canonical path.';; esac
[ "$(cd "$repo" && pwd -P)" = "$repo" ] || fail 'Repository path must not contain aliases or symlinks.'
cd "$repo"
[ -d .git ] && [ ! -L .git ] || fail 'An independent repository with its own .git directory is required; no linked worktrees.'
[ -f .git/config ] && [ ! -L .git/config ] && [ "$(/usr/bin/stat -f %l .git/config)" = 1 ] || fail 'Config must be a normal unshared file.'
git_safe() {
    # Config inspection itself does not execute a monitor; do not contaminate
    # effective values with our defensive per-command override.
    case "$1" in config) git "$@";; *) git -c core.fsmonitor=false "$@";; esac
}
[ "$(git_safe rev-parse --show-toplevel)" = "$repo" ] || fail 'Expected the repository root.'
[ "$(git_safe rev-parse --absolute-git-dir)" = "$repo/.git" ] || fail 'Unexpected Git directory.'
common=$(git_safe rev-parse --git-common-dir)
[ "$(cd "$common" && pwd -P)" = "$repo/.git" ] || fail 'Shared Git directories are not supported.'
[ "$(git_safe rev-parse --is-bare-repository)" = false ] || fail 'Bare repositories are not supported.'
[ ! -e .git/config.lock ] && [ ! -e .git/index.lock ] || fail 'Git is busy; retry after its owner finishes.'
if [ -e .git/index ] || [ -L .git/index ]; then
    [ -f .git/index ] && [ ! -L .git/index ] && [ "$(/usr/bin/stat -f %l .git/index)" = 1 ] || fail 'Index must be a normal unshared file.'
fi
# Do not silently accept worktree config, includes, remote-FS overrides or helper policy.
if git_safe config --local --no-includes --get-regexp '^(include.*\.path|extensions\.worktreeconfig|core\.worktree)$' >/dev/null; then
    fail 'Local includes, worktree config and redirected worktrees need separate review.'
fi
for key in fsmonitor.allowRemote fsmonitor.socketDir; do
    if git_safe config --get-all "$key" >/dev/null; then fail "Remove/review $key separately first."; fi
done
build=$(git version --build-options)
printf '%s\n' "$build" | grep -qx 'feature: fsmonitor--daemon' || fail 'Git lacks the built-in fsmonitor daemon.'
version=$(git version | awk '{print $3}')
printf '%s\n' "$version" | awk -F. '{exit !($1 > 2 || ($1 == 2 && $2 >= 36))}' || fail 'Git 2.36 or later is required for boolean built-in fsmonitor.'
device=$(df -P "$repo" | awk 'NR == 2 {print $1}')
volume=$(/usr/sbin/diskutil info -plist "$device")
[ "$(printf '%s' "$volume" | /usr/bin/plutil -extract FilesystemType raw -o - -)" = apfs ] &&
[ "$(printf '%s' "$volume" | /usr/bin/plutil -extract Internal raw -o - -)" = true ] || fail 'Only local internal APFS is qualified.'
# Permit only absent settings, or our exact existing local pair. Never replace a
# custom helper, inherited setting, duplicate value or partially configured pair.
fsm=$(git_safe config --get-all core.fsmonitor || [ "$?" = 1 ])
uc=$(git_safe config --get-all core.untrackedCache || [ "$?" = 1 ])
new=0
if [ -z "$fsm" ] && [ -z "$uc" ]; then
    # A valueless entry is not absence.
    ! git_safe config --get core.fsmonitor >/dev/null && ! git_safe config --get core.untrackedCache >/dev/null || fail 'Existing empty settings require review.'
    [ "$mode" = enable ] || fail 'Not enabled; use explicit enable first.'
    new=1
else
    [ "$fsm" = true ] && [ "$uc" = true ] &&
    [ "$(git_safe config --local --no-includes --get-all core.fsmonitor)" = true ] &&
    [ "$(git_safe config --local --no-includes --get-all core.untrackedCache)" = true ] || fail 'Existing accelerator settings require separate review; nothing was changed.'
fi
# Tests directory mtime behavior on this repository's actual filesystem.
git_safe update-index --test-untracked-cache
owned_daemon=0
added_fsm=0
added_uc=0
rollback_key() {
    value=$(git_safe config --local --no-includes --get-all "$1" || [ "$?" = 1 ]) || { printf 'Rollback failed: inspect %s.\n' "$1" >&2; return; }
    case "$value" in
        true) git_safe config --local --unset-all "$1" || printf 'Rollback failed: inspect %s.\n' "$1" >&2;;
        '') :;;
        *) printf 'Rollback refused: %s changed; inspect it.\n' "$1" >&2;;
    esac
}
cleanup() {
    result=$?
    trap - 0
    if [ "$result" != 0 ]; then
        # Only additions by this failed invocation are undone. Do not restore a
        # whole config snapshot or stop a daemon that was already running.
        if [ "$added_uc" = 1 ]; then rollback_key core.untrackedCache; fi
        if [ "$added_fsm" = 1 ]; then rollback_key core.fsmonitor; fi
        if [ "$owned_daemon" = 1 ]; then git_safe fsmonitor--daemon stop || printf '%s\n' 'Rollback failed: inspect the repository daemon.' >&2; fi
    fi
    exit "$result"
}
trap cleanup 0
trap 'exit 1' 1 2 15
if git_safe fsmonitor--daemon status >/dev/null 2>&1; then
    : # Preserve a pre-existing daemon even if subsequent setup fails.
else
    daemon_status=$?
    [ "$daemon_status" = 1 ] || fail 'Cannot determine existing daemon ownership.'
    owned_daemon=1
    git_safe fsmonitor--daemon start
fi
if [ "$new" = 1 ]; then
    added_fsm=1
    git_safe config --local --add core.fsmonitor true
    added_uc=1
    git_safe config --local --add core.untrackedCache true
fi
# Deliberately permit an index write, even when called from a read-only harness.
# Never add/reset files, hide untracked paths, or install hooks/helpers.
GIT_OPTIONAL_LOCKS=1 git -c core.hooksPath=/dev/null status --porcelain --untracked-files=normal
printf '%s\n' 'Ordinary-Git acceleration primed. Re-prime after daemon/index invalidation; no automatic refresh is installed.' >&2
