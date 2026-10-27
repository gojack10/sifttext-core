#!/bin/sh
set -eu
cd "$(dirname "$0")/.."
command -v node >/dev/null || { echo 'Node.js is required for the published-KB Git hook.' >&2; exit 1; }
current=$(git config --get core.hooksPath || :)
case "$current" in
  .githooks) ;;
  '')
    if [ -e "$(git rev-parse --git-path hooks/pre-commit)" ]; then
      echo 'Existing pre-commit hook preserved. Chain .githooks/pre-commit from it.' >&2
      exit 1
    fi
    git config --local core.hooksPath .githooks ;;
  *) echo "Existing hooksPath ($current) preserved. Chain .githooks/pre-commit from it." >&2; exit 1 ;;
esac
printf '%s\n' 'Installed the published-KB pre-commit hook for this repository.'
