#!/bin/sh
# Run any build or check command in the same verified tool environment that
# scripts/setup-tools.sh prepared. Repository-local tools are never exported to
# a user shell, so direct Cargo calls need this entry point.
#   usage: sh scripts/with-tools.sh cargo test --locked
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
. "$ROOT/scripts/tool-env.sh"
tool_env_apply
cd "$ROOT"
exec "$@"
